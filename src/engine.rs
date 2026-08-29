use std::time::Duration;

use rquickjs::function::{Func, Opt, Rest};
use rquickjs::loader::{FileResolver, ScriptLoader};
use rquickjs::{
    AsyncContext, AsyncRuntime, CatchResultExt, Class, Ctx, FromJs, Function, Object, Value,
};

use crate::api::{ScriptAuthor, ScriptChat, ScriptLink, ScriptMessage, ScriptRoom, ScriptSession};
use crate::error::ScriptError;

pub const HOOKS: [&str; 18] = [
    "onMessage",
    "onJoin",
    "onLeave",
    "onMemberChange",
    "onRead",
    "onReaction",
    "onFeed",
    "onMetaChange",
    "onSyncJoin",
    "onLinkProfile",
    "onLeft",
    "onLogin",
    "onListening",
    "onKicked",
    "onMoved",
    "onPush",
    "onConnect",
    "onClose",
];

pub struct Script {
    runtime: AsyncRuntime,
    context: AsyncContext,
    name: String,
    defined: Vec<&'static str>,
}

impl Script {
    pub async fn compile(name: &str, code: &str) -> Result<Self, ScriptError> {
        Self::compile_in(name, code, std::path::Path::new(".")).await
    }

    pub async fn compile_in(
        name: &str,
        code: &str,
        directory: &std::path::Path,
    ) -> Result<Self, ScriptError> {
        let runtime = AsyncRuntime::new().map_err(engine_error)?;
        runtime
            .set_loader(
                FileResolver::default().with_path(directory.to_string_lossy().as_ref()),
                ScriptLoader::default().with_extension("js"),
            )
            .await;
        let context = AsyncContext::full(&runtime).await.map_err(engine_error)?;

        let source = code.to_owned();
        // Relative imports resolve against this, so the entry has to know where it lives.
        let filename = directory.join(name).to_string_lossy().into_owned();
        let outcome: Result<Vec<&'static str>, String> = context
            .async_with(async |ctx| {
                Class::<ScriptMessage>::define(&ctx.globals()).map_err(text)?;
                Class::<ScriptChat>::define(&ctx.globals()).map_err(text)?;
                Class::<ScriptAuthor>::define(&ctx.globals()).map_err(text)?;
                Class::<ScriptRoom>::define(&ctx.globals()).map_err(text)?;
                Class::<ScriptLink>::define(&ctx.globals()).map_err(text)?;
                Class::<ScriptSession>::define(&ctx.globals()).map_err(text)?;

                // install additional functions
                install_console(&ctx).map_err(text)?;
                install_timers(&ctx).map_err(text)?;
                crate::api::http::install(&ctx).map_err(text)?;
                crate::api::fs::install(&ctx).map_err(text)?;

                let mut options = rquickjs::context::EvalOptions::default();
                options.global = false;
                options.promise = true;
                options.filename = Some(filename);
                let evaluated: rquickjs::Value = ctx
                    .eval_with_options(source.as_bytes(), options)
                    .catch(&ctx)
                    .map_err(|error| error.to_string())?;
                if let Some(promise) = evaluated.as_promise() {
                    promise
                        .clone()
                        .into_future::<rquickjs::Value>()
                        .await
                        .catch(&ctx)
                        .map_err(|error| error.to_string())?;
                }

                let globals = ctx.globals();
                Ok(HOOKS
                    .into_iter()
                    .filter(|hook| globals.get::<_, Function>(*hook).is_ok())
                    .collect())
            })
            .await;

        let defined = outcome.map_err(|report| ScriptError::Compile {
            name: name.to_owned(),
            report,
        })?;

        Ok(Self {
            runtime,
            context,
            name: name.to_owned(),
            defined,
        })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn defines(&self, hook: &str) -> bool {
        self.defined.contains(&hook)
    }

    pub async fn idle(&self) {
        self.runtime.idle().await;
    }

    #[must_use]
    pub fn drive(&self) -> rquickjs::runtime::DriveFuture {
        self.runtime.drive()
    }

    #[must_use]
    pub fn hooks(&self) -> &[&'static str] {
        &self.defined
    }
}

impl Script {
    pub async fn call<A>(&self, hook: &'static str, args: A) -> Result<(), ScriptError>
    where
        A: for<'js> rquickjs::function::IntoArgs<'js> + Send + 'static,
    {
        if !self.defines(hook) {
            return Ok(());
        }

        let reported: Result<(), String> = self
            .context
            .async_with(async |ctx| {
                let hook: Function = ctx.globals().get(hook).map_err(text)?;
                let returned: rquickjs::Value = hook.call(args).catch(&ctx).map_err(reported)?;

                if let Some(promise) = returned.as_promise() {
                    promise
                        .clone()
                        .into_future::<rquickjs::Value>()
                        .await
                        .catch(&ctx)
                        .map_err(reported)?;
                }
                Ok(())
            })
            .await;

        reported.map_err(ScriptError::Vm)
    }
}

fn install_console(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    let console = Object::new(ctx.clone())?;
    console.set(
        "log",
        Func::from(|args: Rest<Value>| tracing::info!("{}", joined(&args))),
    )?;
    console.set(
        "info",
        Func::from(|args: Rest<Value>| tracing::info!("{}", joined(&args))),
    )?;
    console.set(
        "warn",
        Func::from(|args: Rest<Value>| tracing::warn!("{}", joined(&args))),
    )?;
    console.set(
        "error",
        Func::from(|args: Rest<Value>| tracing::error!("{}", joined(&args))),
    )?;
    console.set(
        "debug",
        Func::from(|args: Rest<Value>| tracing::debug!("{}", joined(&args))),
    )?;
    ctx.globals().set("console", console)
}

fn install_timers<'js>(ctx: &Ctx<'js>) -> rquickjs::Result<()> {
    let globals = ctx.globals();

    globals.set(
        "setTimeout",
        Func::from(
            |ctx: Ctx<'js>, callback: rquickjs::Function<'js>, delay: Opt<u64>| {
                let delay = Duration::from_millis(delay.0.unwrap_or_default());
                let callback = callback.clone();
                ctx.spawn(async move {
                    tokio::time::sleep(delay).await;
                    if let Err(error) = callback.call::<_, Value>(()) {
                        tracing::error!(%error, "timer failed");
                    }
                });
            },
        ),
    )?;

    globals.set(
        "sleep",
        Func::from(|ctx: Ctx<'js>, delay: u64| {
            let (promise, resolve, reject) = ctx.promise()?;
            ctx.spawn(async move {
                tokio::time::sleep(Duration::from_millis(delay)).await;
                let _ = reject;
                let _ = resolve.call::<_, Value>(());
            });
            Ok::<_, rquickjs::Error>(promise)
        }),
    )?;

    Ok(())
}

fn joined(args: &[Value]) -> String {
    args.iter()
        .map(|value| {
            rquickjs::Coerced::<String>::from_js(value.ctx(), value.clone())
                .map(|coerced| coerced.0)
                .unwrap_or_else(|_| format!("{:?}", value.type_of()))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
impl Script {
    pub(crate) async fn probe(&self, expression: &str) -> String {
        let source = format!("String({expression})");
        self.context
            .async_with(async |ctx| {
                ctx.eval::<String, _>(source.as_bytes())
                    .unwrap_or_else(|error| error.to_string())
            })
            .await
    }
}

fn engine_error(error: impl std::fmt::Display) -> ScriptError {
    ScriptError::Rune(error.to_string())
}

fn reported(error: rquickjs::CaughtError<'_>) -> String {
    if let rquickjs::CaughtError::Exception(exception) = &error {
        let mut rendered = exception.message().unwrap_or_else(|| error.to_string());
        if let Some(stack) = exception.stack() {
            rendered.push('\n');
            rendered.push_str(stack.trim_end());
        }
        return rendered;
    }
    error.to_string()
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rquickjs::class::Trace;
    use rquickjs::{Class, JsLifetime};

    use super::{HOOKS, Script};

    #[derive(Clone, Trace, JsLifetime)]
    #[rquickjs::class(rename = "Inner")]
    pub struct Inner {
        #[qjs(get)]
        pub id: i64,
    }

    #[rquickjs::methods]
    impl Inner {
        fn touch(&self, note: String) -> String {
            format!("{}:{note}", self.id)
        }

        async fn give(&self) -> String {
            super::super::api::id_text(3_915_793_272_451_299_329)
        }

        async fn take(&self, id: String, note: String) -> rquickjs::Result<String> {
            Ok(format!("{}:{note}", super::super::api::id_of(&id)?))
        }
    }

    #[derive(Clone, Trace, JsLifetime)]
    #[rquickjs::class(rename = "Outer")]
    pub struct Outer {
        #[qjs(skip_trace)]
        inner: Inner,
    }

    #[rquickjs::methods]
    impl Outer {
        #[qjs(get, rename = "inner")]
        fn inner_js(&self) -> Inner {
            self.inner.clone()
        }
    }

    #[tokio::test]
    async fn a_nested_class_keeps_its_methods() {
        let script = Script::compile("t", "").await.unwrap();

        let reported: String = script
            .context
            .async_with(async |ctx| {
                Class::<Inner>::define(&ctx.globals()).unwrap();
                Class::<Outer>::define(&ctx.globals()).unwrap();
                let outer = Class::instance(
                    ctx.clone(),
                    Outer {
                        inner: Inner { id: 7 },
                    },
                )
                .unwrap();
                ctx.globals().set("outer", outer).unwrap();
                ctx.eval::<String, _>(
                    "String(typeof outer.inner.touch) + ' ' + String(outer.inner.id)".as_bytes(),
                )
                .unwrap_or_else(|error| error.to_string())
            })
            .await;

        assert_eq!(reported, "function 7", "a getter dropped the methods");
    }

    #[tokio::test]
    async fn a_script_says_which_hooks_it_defines() {
        let script = Script::compile("t", "globalThis.onMessage = async () => {};")
            .await
            .unwrap();

        assert!(script.defines("onMessage"));
        assert!(!script.defines("onJoin"));
    }

    #[tokio::test]
    async fn a_script_without_hooks_defines_none() {
        let script = Script::compile("t", "const x = 1;").await.unwrap();

        assert!(script.hooks().is_empty());
    }

    #[tokio::test]
    async fn every_named_hook_is_findable() {
        let source: String = HOOKS
            .iter()
            .map(|hook| format!("globalThis.{hook} = async () => {{}};\n"))
            .collect();

        let script = Script::compile("t", &source).await.unwrap();

        assert_eq!(script.hooks().len(), HOOKS.len());
    }

    #[tokio::test]
    async fn console_is_there_because_quickjs_does_not_ship_one() {
        let script = Script::compile(
            "t",
            "globalThis.onMessage = () => { console.log('x', 1); console.error('y'); };",
        )
        .await
        .unwrap();

        script.call("onMessage", ()).await.unwrap();
    }

    #[tokio::test]
    async fn a_hook_that_throws_is_reported() {
        let script = Script::compile("t", "globalThis.onMessage = () => { nope(); };")
            .await
            .unwrap();

        assert!(script.call("onMessage", ()).await.is_err());
    }

    #[tokio::test]
    async fn a_rejected_promise_is_reported() {
        let script = Script::compile("t", "globalThis.onMessage = async () => { nope(); };")
            .await
            .unwrap();

        assert!(
            script.call("onMessage", ()).await.is_err(),
            "an async hook swallowed its own failure"
        );
    }

    #[tokio::test]
    async fn a_script_imports_a_module_beside_it() {
        let dir = std::path::PathBuf::from("target/fixtures/import");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::write(dir.join("lib/dice.js"), "export const roll = (n) => n;").unwrap();
        let entry = "import { roll } from './lib/dice.js';\n\
                     globalThis.onMessage = () => { if (roll(20) < 1) throw 'bad'; };";

        let script = Script::compile_in("main.js", entry, &dir).await.unwrap();

        assert!(script.defines("onMessage"), "a module lost its hooks");
        script.call("onMessage", ()).await.unwrap();
    }

    #[tokio::test]
    async fn what_quickjs_ships_on_its_own() {
        let script = Script::compile("probe", "").await.unwrap();

        for present in [
            "JSON", "Math", "Promise", "Date", "RegExp", "Map", "Set", "BigInt",
        ] {
            assert_ne!(
                script.probe(&format!("typeof {present}")).await,
                "undefined",
                "{present} is missing",
            );
        }

        // What we filled in ourselves.
        for bound in ["console", "setTimeout", "sleep", "fetch", "fs"] {
            assert_ne!(
                script.probe(&format!("typeof {bound}")).await,
                "undefined",
                "{bound} was dropped",
            );
        }

        // Still host territory, still unbound.
        for absent in ["TextEncoder", "URL", "crypto"] {
            assert_eq!(
                script.probe(&format!("typeof {absent}")).await,
                "undefined",
                "{absent} arrived on its own",
            );
        }
    }

    #[tokio::test]
    async fn sleep_actually_waits() {
        let script = Script::compile(
            "t",
            "globalThis.onMessage = async () => { globalThis.done = false; await sleep(60); globalThis.done = true; };",
        )
        .await
        .unwrap();

        let started = std::time::Instant::now();
        script.call("onMessage", ()).await.unwrap();

        assert!(started.elapsed().as_millis() >= 60, "it returned early");
        assert_eq!(script.probe("globalThis.done").await, "true");
    }

    #[tokio::test]
    async fn a_timer_runs_after_the_hook_returns() {
        let script = Script::compile(
            "t",
            "globalThis.fired = false;\n\
             globalThis.onMessage = () => { setTimeout(() => { globalThis.fired = true; }, 20); };",
        )
        .await
        .unwrap();

        script.call("onMessage", ()).await.unwrap();
        assert_eq!(
            script.probe("globalThis.fired").await,
            "false",
            "it ran early"
        );

        script.idle().await;
        assert_eq!(
            script.probe("globalThis.fired").await,
            "true",
            "it never ran"
        );
    }

    #[tokio::test]
    async fn a_timer_fires_while_the_host_waits() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let script = Script::compile(
                    "t",
                    "globalThis.fired = false;\n\
                     setTimeout(() => { globalThis.fired = true; }, 20);",
                )
                .await
                .unwrap();

                tokio::task::spawn_local(script.drive());
                tokio::time::sleep(Duration::from_millis(150)).await;

                assert_eq!(
                    script.probe("globalThis.fired").await,
                    "true",
                    "배경 작업이 호스트를 기다리다 멈췄다",
                );
            })
            .await;
    }

    #[tokio::test]
    async fn a_script_keeps_its_own_state_on_disk() {
        let path = std::env::temp_dir().join("script-engine-state/sheet.json");
        let _ = std::fs::remove_file(&path);
        let path = path.display().to_string();

        let script = Script::compile(
            "t",
            &format!(
                "globalThis.onMessage = async () => {{\n\
                   await fs.write({path:?}, JSON.stringify({{ hp: 10 }}));\n\
                   const back = JSON.parse(await fs.read({path:?}));\n\
                   globalThis.hp = back.hp;\n\
                 }};"
            ),
        )
        .await
        .unwrap();

        script.call("onMessage", ()).await.unwrap();

        assert_eq!(script.probe("globalThis.hp").await, "10");
        // `write` made the directory rather than failing on a missing one.
        assert!(std::path::Path::new(&path).exists());
    }

    #[tokio::test]
    async fn a_missing_file_rejects_rather_than_returning_nothing() {
        let script = Script::compile(
            "t",
            "globalThis.onMessage = async () => {\n\
               try { await fs.read('/nope/nope'); globalThis.caught = 'no'; }\n\
               catch (error) { globalThis.caught = 'yes'; }\n\
             };",
        )
        .await
        .unwrap();

        script.call("onMessage", ()).await.unwrap();

        assert_eq!(script.probe("globalThis.caught").await, "yes");
    }

    #[tokio::test]
    async fn a_log_id_survives_a_round_trip() {
        let script = Script::compile("t", "").await.unwrap();

        let reported: String = script
            .context
            .async_with(async |ctx| {
                Class::<Inner>::define(&ctx.globals()).unwrap();
                let inner = Class::instance(ctx.clone(), Inner { id: 1 }).unwrap();
                ctx.globals().set("inner", inner).unwrap();
                let source = "(async () => {\n\
                    const id = await inner.give();\n\
                    return typeof id + ' ' + String(await inner.take(id, 'x'));\n\
                  })()";
                match ctx.eval::<rquickjs::Promise, _>(source.as_bytes()) {
                    Ok(promise) => promise
                        .into_future::<String>()
                        .await
                        .unwrap_or_else(|error| error.to_string()),
                    Err(error) => error.to_string(),
                }
            })
            .await;

        assert_eq!(
            reported, "string 3915793272451299329:x",
            "a log id changed on the way through",
        );
    }

    #[tokio::test]
    async fn console_renders_more_than_strings() {
        let script = Script::compile("t", "").await.unwrap();

        assert_eq!(script.probe("String(42)").await, "42");
        assert_eq!(
            script.probe("String(new Error('boom'))").await,
            "Error: boom"
        );
        assert_eq!(script.probe("String({ a: 1 })").await, "[object Object]");
    }

    #[tokio::test]
    async fn a_broken_script_reports_where_it_broke() {
        let refused = Script::compile("t", "const = ;").await;

        let Err(error) = refused else {
            panic!("a broken script compiled");
        };
        assert!(error.is_compile(), "{error}");
    }
}
