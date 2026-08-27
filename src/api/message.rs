use kakao_loco_client::core::command::chat::ChatLog;
use kakao_loco_client::prelude::*;
use rquickjs::JsLifetime;
use rquickjs::class::Trace;

use super::{ScriptAuthor, ScriptChat, failed};

#[derive(Clone, Trace, JsLifetime)]
#[rquickjs::class(rename = "Message")]
pub struct ScriptMessage {
    #[qjs(get)]
    pub text: String,
    #[qjs(skip_trace)]
    pub author: ScriptAuthor,
    #[qjs(skip_trace)]
    pub chat: ScriptChat,
    #[qjs(get)]
    pub log_id: i64,
    #[qjs(skip_trace)]
    log: ChatLog,
}

impl ScriptMessage {
    #[must_use]
    pub fn new(message: &Message<'_>) -> Self {
        Self {
            text: message.text().to_owned(),
            author: ScriptAuthor::new(message.author()),
            chat: ScriptChat::new(&message.chat()),
            log_id: message.log_id(),
            log: message.log(),
        }
    }
}

#[rquickjs::methods]
impl ScriptMessage {
    #[qjs(get, rename = "author")]
    fn author_js(&self) -> ScriptAuthor {
        self.author.clone()
    }

    #[qjs(get, rename = "chat")]
    fn chat_js(&self) -> ScriptChat {
        self.chat.clone()
    }

    async fn say(&self, text: String) -> rquickjs::Result<i64> {
        self.chat
            .chat
            .write(&text)
            .await
            .map(|log| log.log_id)
            .map_err(failed)
    }

    async fn reply(&self, text: String) -> rquickjs::Result<i64> {
        self.chat
            .chat
            .reply(&self.log, &text)
            .await
            .map(|log| log.log_id)
            .map_err(failed)
    }

    async fn react(&self, reaction: i32) -> rquickjs::Result<()> {
        let reaction = ReactionType::new(reaction)
            .ok_or_else(|| failed(format!("{reaction} 은 없는 반응이다")))?;
        self.chat
            .chat
            .react(self.log_id, reaction)
            .await
            .map_err(failed)
    }

    #[qjs(rename = "cancelReaction")]
    async fn cancel_reaction(&self) -> rquickjs::Result<()> {
        self.chat
            .chat
            .cancel_reaction(self.log_id)
            .await
            .map_err(failed)
    }

    async fn edit(&self, text: String) -> rquickjs::Result<()> {
        self.chat
            .chat
            .edit(&self.log, &text)
            .await
            .map(drop)
            .map_err(failed)
    }

    async fn delete(&self) -> rquickjs::Result<bool> {
        self.chat
            .chat
            .delete(self.log_id)
            .await
            .map(|log| log.is_some())
            .map_err(failed)
    }

    async fn hide(&self) -> rquickjs::Result<String> {
        self.chat.chat.hide(&[self.log_id]).await.map_err(failed)
    }

    #[qjs(rename = "toString")]
    fn to_string_js(&self) -> String {
        format!("{}: {}", self.author.name, self.text)
    }
}
