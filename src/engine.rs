use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rune::runtime::RuntimeContext;
use rune::{Diagnostics, Source, Sources, Unit, Vm};

use crate::api::{self, ScriptMessage};
use crate::error::ScriptError;

pub const ON_MESSAGE: &str = "on_message";

pub const EVAL_BUDGET: usize = 100_000;

pub const EVAL_MEMORY: usize = 4 * 1024 * 1024;

pub const EVAL_DEPTH_LIMIT: usize = 1;

/// A spawned task starts its own depth scope, so without a ceiling a script could fork forever.
pub const SPAWN_LIMIT: usize = 32;

static SPAWNED: AtomicUsize = AtomicUsize::new(0);

tokio::task_local! {
    static EVAL_DEPTH: usize;
}

#[derive(Clone)]
pub struct Script {
    runtime: Arc<RuntimeContext>,
    unit: Arc<Unit>,
    handles_messages: bool,
}

impl Script {
    pub fn compile(name: &str, code: &str) -> Result<Self, ScriptError> {
        let mut context = rune::Context::with_default_modules().map_err(rune_error)?;
        for module in extras().map_err(rune_error)? {
            context.install(module).map_err(rune_error)?;
        }
        context
            .install(api::module().map_err(rune_error)?)
            .map_err(rune_error)?;
        let runtime = Arc::new(context.runtime().map_err(rune_error)?);

        let mut sources = Sources::new();
        sources
            .insert(Source::new(name, code).map_err(rune_error)?)
            .map_err(rune_error)?;

        let mut diagnostics = Diagnostics::new();
        let built = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        let unit = Arc::new(built.map_err(|_| ScriptError::Compile {
            name: name.to_owned(),
            report: report(&diagnostics, &sources),
        })?);
        let handles_messages = Vm::new(runtime.clone(), unit.clone())
            .lookup_function([ON_MESSAGE])
            .is_ok();

        Ok(Self {
            runtime,
            unit,
            handles_messages,
        })
    }

    #[must_use]
    pub fn handles_messages(&self) -> bool {
        self.handles_messages
    }

    pub async fn on_message(&self, message: ScriptMessage) -> Result<(), ScriptError> {
        if !self.handles_messages {
            return Ok(());
        }
        let mut vm = Vm::new(self.runtime.clone(), self.unit.clone());
        vm.async_call([ON_MESSAGE], (message,))
            .await
            .map(|_| ())
            .map_err(|error| ScriptError::Vm(error.to_string()))
    }
}

/// Anything that reaches the host machine or the network is left out on purpose.
fn extras() -> Result<Vec<rune::Module>, rune::ContextError> {
    Ok(vec![
        rune_modules::json::module(false)?,
        rune_modules::time::module(false)?,
        rune_modules::rand::module(false)?,
        rune_modules::base64::module(false)?,
    ])
}

fn rune_error(error: impl std::fmt::Display) -> ScriptError {
    ScriptError::Rune(error.to_string())
}

fn report(diagnostics: &Diagnostics, sources: &Sources) -> String {
    let mut rendered = rune::termcolor::Buffer::no_color();
    if diagnostics.emit(&mut rendered, sources).is_err() {
        return "스크립트를 컴파일하지 못했다".to_owned();
    }
    String::from_utf8_lossy(rendered.as_slice())
        .trim()
        .to_owned()
}

pub async fn eval(message: ScriptMessage, code: &str) -> Result<String, ScriptError> {
    let depth = EVAL_DEPTH.try_with(|depth| *depth).unwrap_or(0);
    if depth >= EVAL_DEPTH_LIMIT {
        return Err(ScriptError::Nested);
    }
    EVAL_DEPTH
        .scope(depth + 1, run(code, "msg", (message,)))
        .await
}

/// A rune vm is not `Send`, so the caller has to be inside a [`tokio::task::LocalSet`].
pub fn spawn(work: rune::runtime::Future) -> Result<(), ScriptError> {
    let slot = Slot::claim()?;
    tokio::task::spawn_local(async move {
        let _slot = slot;
        if let Err(error) = rune::runtime::budget::with(EVAL_BUDGET, work)
            .await
            .into_result()
        {
            tracing::error!(%error, "background script failed");
        }
    });
    Ok(())
}

/// Holds one background slot. Dropping it gives the slot back, so a script that panics
/// or is cancelled does not take one with it.
struct Slot;

impl Slot {
    fn claim() -> Result<Self, ScriptError> {
        if SPAWNED.fetch_add(1, Ordering::Relaxed) >= SPAWN_LIMIT {
            SPAWNED.fetch_sub(1, Ordering::Relaxed);
            return Err(ScriptError::Crowded { limit: SPAWN_LIMIT });
        }
        Ok(Self)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        SPAWNED.fetch_sub(1, Ordering::Relaxed);
    }
}

#[must_use]
pub fn spawned() -> usize {
    SPAWNED.load(Ordering::Relaxed)
}

async fn run(
    code: &str,
    params: &str,
    args: impl rune::runtime::GuardedArgs + Send,
) -> Result<String, ScriptError> {
    let wrapped = format!("pub async fn main({params}) {{ {code} }}");
    let script = Script::compile("eval", &wrapped)?;
    let mut vm = Vm::new(script.runtime.clone(), script.unit.clone());

    let outcome = rune::runtime::budget::with(EVAL_BUDGET, vm.async_call(["main"], args)).await;

    match outcome {
        Ok(value) => Ok(vm.with(|| format!("{value:?}"))),
        Err(error) => Err(ScriptError::Vm(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::{SPAWN_LIMIT, Script};
    use crate::error::ScriptError;

    #[test]
    fn a_script_that_handles_messages_says_so() {
        let script = Script::compile("t", "pub async fn on_message(msg) {}").unwrap();

        assert!(script.handles_messages());
    }

    #[test]
    fn a_script_without_the_hook_is_still_valid() {
        let script = Script::compile("t", "pub fn other() {}").unwrap();

        assert!(!script.handles_messages());
    }

    #[test]
    fn a_broken_script_reports_where_it_broke() {
        let refused = Script::compile("t", "pub async fn on_message(msg) { msg. }");

        let Err(error) = refused else {
            panic!("a broken script compiled");
        };
        assert!(error.is_compile(), "{error}");
    }

    async fn eval(code: &str) -> Result<String, ScriptError> {
        super::run(code, "", ()).await
    }

    #[tokio::test]
    async fn an_expression_evaluates_to_its_value() {
        assert_eq!(eval("1 + 2").await.unwrap(), "3");
        assert_eq!(eval("let x = 4; x * 5").await.unwrap(), "20");
    }

    #[tokio::test]
    async fn something_awaited_still_evaluates() {
        assert_eq!(eval("async { 7 }.await").await.unwrap(), "7");
    }

    #[tokio::test]
    async fn a_runaway_loop_is_cut_off_rather_than_hanging_the_bot() {
        let refused = eval("let n = 0; while true { n += 1 } n").await;

        let Err(error) = refused else {
            panic!("an endless loop returned");
        };
        assert!(
            matches!(error, ScriptError::Vm(_)),
            "a budget stop is not a compile failure: {error}"
        );
    }

    #[tokio::test]
    async fn a_collection_prints_its_contents_rather_than_its_address() {
        let rendered = eval("[1, 2, 3]").await.unwrap();

        assert!(rendered.contains('1'), "{rendered}");
        assert!(!rendered.contains("0x"), "{rendered}");
    }

    #[tokio::test]
    async fn a_reaction_can_be_named_rather_than_numbered() {
        assert_eq!(eval("bot::HEART").await.unwrap(), "1");
        assert_eq!(eval("bot::CANCEL").await.unwrap(), "0");
    }

    #[tokio::test]
    async fn json_round_trips() {
        let rendered = eval(r#"json::from_string("{\"a\":1}")?["a"]"#)
            .await
            .unwrap();

        assert_eq!(rendered, "1");
    }

    #[tokio::test]
    async fn a_script_can_wait() {
        assert!(
            eval("time::sleep(time::Duration::from_millis(1)).await; 1")
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn a_script_can_roll_dice() {
        let rolled = eval("rand::int_range(1, 7)?").await.unwrap();

        let rolled: i64 = rolled.parse().unwrap();
        assert!((1..7).contains(&rolled), "{rolled}");
    }

    #[tokio::test]
    async fn nothing_reaches_the_network_or_the_disk() {
        for shut in [
            "http::get(\"http://x\")",
            "fs::read_to_string(\"/etc/passwd\")",
            "process::Command::new(\"sh\")",
        ] {
            assert!(eval(shut).await.is_err(), "{shut} resolved");
        }
    }

    #[test]
    fn the_background_can_only_hold_so_many() {
        let held: Vec<_> = (0..SPAWN_LIMIT)
            .map(|_| super::Slot::claim().expect("under the ceiling"))
            .collect();

        assert_eq!(super::spawned(), SPAWN_LIMIT);
        assert!(
            super::Slot::claim().is_err(),
            "the ceiling let one more through"
        );

        drop(held);
        assert_eq!(super::spawned(), 0, "a finished script freed nothing");
    }

    #[test]
    fn a_script_that_unwinds_still_frees_its_slot() {
        let before = super::spawned();

        let _ = std::panic::catch_unwind(|| {
            let _slot = super::Slot::claim().expect("under the ceiling");
            panic!("a script blew up");
        });

        assert_eq!(super::spawned(), before);
    }

    #[tokio::test]
    async fn code_that_does_not_compile_says_so() {
        assert!(eval("1 +").await.is_err());
    }

    #[tokio::test]
    async fn eval_cannot_reach_the_host_machine_either() {
        assert!(eval("std::fs::read(\"/etc/passwd\")").await.is_err());
    }

    #[test]
    fn a_script_cannot_reach_the_host_machine() {
        let refused = Script::compile("t", "pub fn go() { std::fs::read(\"/etc/passwd\") }");

        assert!(refused.is_err());
    }
}
