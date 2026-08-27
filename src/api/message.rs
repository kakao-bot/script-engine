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
}
