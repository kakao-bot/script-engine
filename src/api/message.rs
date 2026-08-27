use kakao_loco_client::core::command::chat::ChatLog;
use kakao_loco_client::core::ids::LogId;
use kakao_loco_client::prelude::*;
use rune::alloc::fmt::TryWrite;
use rune::runtime::{Formatter, Ref, VmResult};
use rune::vm_write;

use super::{ScriptAuthor, ScriptChat, text_of};

#[derive(Clone, rune::Any)]
#[rune(item = ::bot)]
pub struct ScriptMessage {
    #[rune(get)]
    pub text: String,
    #[rune(get)]
    pub author: ScriptAuthor,
    #[rune(get)]
    pub chat: ScriptChat,
    #[rune(get)]
    pub log_id: i64,
    #[rune(get)]
    pub thread_id: i64,
    #[rune(get)]
    pub kind: i32,
    #[rune(get)]
    pub attachment: String,
    pub(crate) log: ChatLog,
}

impl rune::alloc::clone::TryClone for ScriptMessage {
    fn try_clone(&self) -> Result<Self, rune::alloc::Error> {
        Ok(self.clone())
    }
}

impl ScriptMessage {
    #[must_use]
    pub fn new(message: &Message<'_>) -> Self {
        let log = message.log();
        Self {
            text: message.text().to_owned(),
            author: ScriptAuthor::new(message.author()),
            chat: ScriptChat::new(&message.chat()),
            log_id: message.log_id(),
            thread_id: message.thread_id().unwrap_or_default(),
            kind: message.kind().wire_value(),
            attachment: log.attachment.clone(),
            log,
        }
    }

    #[rune::function(keep, instance, protocol = DISPLAY_FMT)]
    fn display_fmt(&self, f: &mut Formatter) -> VmResult<()> {
        vm_write!(f, "{}: {}", self.author.display, self.text)
    }

    #[rune::function(keep, instance, protocol = DEBUG_FMT)]
    fn debug_fmt(&self, f: &mut Formatter) -> VmResult<()> {
        vm_write!(
            f,
            "ScriptMessage {{ author: {:?}, text: {:?}, log_id: {}, chat_id: {}, thread_id: {}, kind: {}, attachment: {:?} }}",
            self.author.display,
            self.text,
            self.log_id,
            self.chat.id,
            self.thread_id,
            self.kind,
            self.attachment
        )
    }
}

#[rune::function(instance, keep)]
async fn say(this: Ref<ScriptMessage>, text: String) -> Result<i64, String> {
    this.chat
        .chat
        .write(&text)
        .await
        .map(|log| log.log_id)
        .map_err(text_of)
}

#[rune::function(instance, keep)]
async fn reply(this: Ref<ScriptMessage>, text: String) -> Result<i64, String> {
    this.chat
        .chat
        .reply(&this.log, &text)
        .await
        .map(|log| log.log_id)
        .map_err(text_of)
}

#[rune::function(instance, keep)]
async fn edit(this: Ref<ScriptMessage>, log_id: LogId, text: String) -> Result<(), String> {
    let Some(log) = this.chat.chat.log(log_id).await.map_err(text_of)? else {
        return Err(format!("로그 {log_id} 이 없다"));
    };
    this.chat
        .chat
        .edit(&log, &text)
        .await
        .map(drop)
        .map_err(text_of)
}

#[rune::function(instance, keep)]
async fn delete(this: Ref<ScriptMessage>) -> Result<bool, String> {
    this.chat
        .chat
        .delete(this.log_id)
        .await
        .map(|log| log.is_some())
        .map_err(text_of)
}

#[rune::function(instance, keep)]
async fn react(this: Ref<ScriptMessage>, reaction: i32) -> Result<(), String> {
    let reaction =
        ReactionType::new(reaction).ok_or_else(|| format!("{reaction} 은 없는 반응이다"))?;
    this.chat
        .chat
        .react(this.log_id, reaction)
        .await
        .map_err(text_of)
}

pub fn install(module: &mut rune::Module) -> Result<(), rune::ContextError> {
    module.ty::<ScriptMessage>()?;
    module.function_meta(say__meta)?;
    module.function_meta(reply__meta)?;
    module.function_meta(edit__meta)?;
    module.function_meta(delete__meta)?;
    module.function_meta(react__meta)?;
    module.function_meta(ScriptMessage::display_fmt__meta)?;
    module.function_meta(ScriptMessage::debug_fmt__meta)?;
    Ok(())
}
