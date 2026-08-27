use kakao_loco_client::core::ids::LogId;
use kakao_loco_client::prelude::*;
use rquickjs::JsLifetime;
use rquickjs::class::Trace;

use super::{ScriptAuthor, ScriptSession, failed};

#[derive(Clone, Trace, JsLifetime)]
#[rquickjs::class(rename = "Chat")]
pub struct ScriptChat {
    #[qjs(get)]
    pub id: i64,
    #[qjs(get)]
    pub is_open_chat: bool,
    #[qjs(get)]
    pub link_id: i64,
    #[qjs(skip_trace)]
    pub(crate) chat: Chat,
}

impl ScriptChat {
    #[must_use]
    pub fn new(chat: &Chat) -> Self {
        Self {
            id: chat.id(),
            is_open_chat: chat.is_open_chat().unwrap_or_default(),
            link_id: chat.link_id().unwrap_or_default(),
            chat: chat.clone(),
        }
    }
}

#[rquickjs::methods]
impl ScriptChat {
    async fn write(&self, text: String) -> rquickjs::Result<i64> {
        self.chat
            .write(&text)
            .await
            .map(|log| log.log_id)
            .map_err(failed)
    }

    async fn delete(&self, log_id: LogId) -> rquickjs::Result<bool> {
        self.chat
            .delete(log_id)
            .await
            .map(|log| log.is_some())
            .map_err(failed)
    }

    async fn hide(&self, log_id: LogId) -> rquickjs::Result<String> {
        self.chat.hide(&[log_id]).await.map_err(failed)
    }

    async fn react(&self, log_id: LogId, reaction: i32) -> rquickjs::Result<()> {
        let reaction = ReactionType::new(reaction)
            .ok_or_else(|| failed(format!("{reaction} 은 없는 반응이다")))?;
        self.chat.react(log_id, reaction).await.map_err(failed)
    }

    async fn members(&self) -> rquickjs::Result<Vec<ScriptAuthor>> {
        Ok(self
            .chat
            .members()
            .await
            .map_err(failed)?
            .into_iter()
            .map(ScriptAuthor::new)
            .collect())
    }

    async fn leave(&self) -> rquickjs::Result<()> {
        self.chat.leave().await.map_err(failed)
    }

    fn session(&self) -> ScriptSession {
        ScriptSession::new(self.chat.session().clone())
    }

    #[qjs(rename = "writeInThread")]
    async fn write_in_thread(&self, thread_id: i64, text: String) -> rquickjs::Result<i64> {
        self.chat
            .write_in_thread(thread_id, &text)
            .await
            .map(|log| log.log_id)
            .map_err(failed)
    }

    #[qjs(rename = "cancelReaction")]
    async fn cancel_reaction(&self, log_id: LogId) -> rquickjs::Result<()> {
        self.chat.cancel_reaction(log_id).await.map_err(failed)
    }

    #[qjs(rename = "kickLeave")]
    async fn kick_leave(&self) -> rquickjs::Result<i64> {
        self.chat.kick_leave().await.map_err(failed)
    }

    #[qjs(rename = "toString")]
    fn to_string_js(&self) -> String {
        format!("Chat({})", self.id)
    }
}
