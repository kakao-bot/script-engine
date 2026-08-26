use std::sync::Arc;

use rune::runtime::RuntimeContext;
use rune::{Diagnostics, Source, Sources, Unit, Vm};

use crate::api::{self, ScriptMessage};

pub const ON_MESSAGE: &str = "on_message";

pub const EVAL_BUDGET: usize = 100_000;

pub const EVAL_MEMORY: usize = 4 * 1024 * 1024;

pub const EVAL_DEPTH_LIMIT: usize = 1;

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
    pub fn compile(name: &str, code: &str) -> Result<Self, String> {
        let mut context = rune::Context::with_default_modules().map_err(text)?;
        context
            .install(api::module().map_err(text)?)
            .map_err(text)?;
        let runtime = Arc::new(context.runtime().map_err(text)?);

        let mut sources = Sources::new();
        sources
            .insert(Source::new(name, code).map_err(text)?)
            .map_err(text)?;

        let mut diagnostics = Diagnostics::new();
        let built = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        let unit = Arc::new(built.map_err(|_| report(&diagnostics, &sources))?);
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

    pub async fn on_message(&self, message: ScriptMessage) -> Result<(), String> {
        if !self.handles_messages {
            return Ok(());
        }
        let mut vm = Vm::new(self.runtime.clone(), self.unit.clone());
        vm.async_call([ON_MESSAGE], (message,))
            .await
            .map(|_| ())
            .map_err(text)
    }
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
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

pub async fn eval(message: ScriptMessage, code: &str) -> Result<String, String> {
    let depth = EVAL_DEPTH.try_with(|depth| *depth).unwrap_or(0);
    if depth >= EVAL_DEPTH_LIMIT {
        return Err("eval 안에서 다시 eval 할 수 없다".to_owned());
    }
    EVAL_DEPTH
        .scope(depth + 1, run(code, "msg", (message,)))
        .await
}

async fn run(
    code: &str,
    params: &str,
    args: impl rune::runtime::GuardedArgs + Send,
) -> Result<String, String> {
    let wrapped = format!("pub async fn main({params}) {{ {code} }}");
    let script = Script::compile("eval", &wrapped)?;
    let mut vm = Vm::new(script.runtime.clone(), script.unit.clone());

    let outcome = rune::runtime::budget::with(EVAL_BUDGET, vm.async_call(["main"], args)).await;

    match outcome {
        Ok(value) => Ok(vm.with(|| format!("{value:?}"))),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::Script;

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

        let Err(reported) = refused else {
            panic!("a broken script compiled");
        };
        assert!(!reported.is_empty(), "the author needs something to read");
    }

    async fn eval(code: &str) -> Result<String, String> {
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

        let Err(reported) = refused else {
            panic!("an endless loop returned");
        };
        assert!(!reported.is_empty());
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
