use rune::runtime::Ref;

use super::ScriptMessage;

#[rune::function]
async fn eval(message: Ref<ScriptMessage>, code: String) -> String {
    let message = ScriptMessage::clone(&message);
    match crate::engine::eval(message, &code).await {
        Ok(value) => value,
        Err(error) => error.to_string(),
    }
}

#[rune::function]
fn spawn(work: rune::runtime::Future) -> Result<(), String> {
    crate::engine::spawn(work).map_err(|error| error.to_string())
}

#[rune::function]
fn spawned() -> i64 {
    i64::try_from(crate::engine::spawned()).unwrap_or_default()
}

pub fn install(module: &mut rune::Module) -> Result<(), rune::ContextError> {
    module.function_meta(eval)?;
    module.function_meta(spawn)?;
    module.function_meta(spawned)?;
    Ok(())
}
