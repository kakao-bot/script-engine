use rquickjs::function::{Func, Rest};
use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Class, Ctx, Function, Object, Value};

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
    context: AsyncContext,
    name: String,
    defined: Vec<&'static str>,
}

impl Script {
    pub async fn compile(name: &str, code: &str) -> Result<Self, ScriptError> {
        let runtime = AsyncRuntime::new().map_err(engine_error)?;
        let context = AsyncContext::full(&runtime).await.map_err(engine_error)?;

        let source = code.to_owned();
        let outcome: Result<Vec<&'static str>, String> = context
            .with(|ctx| {
                Class::<ScriptMessage>::define(&ctx.globals()).map_err(text)?;
                Class::<ScriptChat>::define(&ctx.globals()).map_err(text)?;
                Class::<ScriptAuthor>::define(&ctx.globals()).map_err(text)?;
                Class::<ScriptRoom>::define(&ctx.globals()).map_err(text)?;
                Class::<ScriptLink>::define(&ctx.globals()).map_err(text)?;
                Class::<ScriptSession>::define(&ctx.globals()).map_err(text)?;
                install_console(&ctx).map_err(text)?;

                ctx.eval::<(), _>(source.as_bytes())
                    .catch(&ctx)
                    .map_err(|error| error.to_string())?;

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

    #[must_use]
    pub fn hooks(&self) -> &[&'static str] {
        &self.defined
    }
}

impl Script {
    /// Calls a hook and, when it returns a promise, waits for it to settle.
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
                let returned: rquickjs::Value = hook.call(args).catch(&ctx).map_err(text)?;

                if let Some(promise) = returned.as_promise() {
                    promise
                        .clone()
                        .into_future::<rquickjs::Value>()
                        .await
                        .catch(&ctx)
                        .map_err(text)?;
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

fn joined(args: &[Value]) -> String {
    args.iter()
        .map(|value| {
            value
                .clone()
                .into_string()
                .and_then(|text| text.to_string().ok())
                .unwrap_or_else(|| format!("{value:?}"))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn engine_error(error: impl std::fmt::Display) -> ScriptError {
    ScriptError::Rune(error.to_string())
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::{HOOKS, Script};

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
    async fn a_broken_script_reports_where_it_broke() {
        let refused = Script::compile("t", "const = ;").await;

        let Err(error) = refused else {
            panic!("a broken script compiled");
        };
        assert!(error.is_compile(), "{error}");
    }
}
