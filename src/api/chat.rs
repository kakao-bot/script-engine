use kakao_loco_client::core::command::chat::ChatLog;
use kakao_loco_client::prelude::*;
use rquickjs::JsLifetime;
use rquickjs::class::Trace;

use super::{ScriptAuthor, ScriptSession, failed, id_of, id_text};

#[derive(Clone, Trace, JsLifetime)]
#[rquickjs::class(rename = "Chat")]
pub struct ScriptChat {
    #[qjs(get)]
    pub id: String,
    #[qjs(get)]
    pub is_open_chat: bool,
    #[qjs(get)]
    pub link_id: String,
    #[qjs(skip_trace)]
    pub(crate) chat: Chat,
}

impl ScriptChat {
    async fn fetch(&self, log_id: &str) -> rquickjs::Result<ChatLog> {
        self.chat
            .log(id_of(log_id)?)
            .await
            .map_err(failed)?
            .ok_or_else(|| failed(format!("로그 {log_id} 이(가) 없다")))
    }

    #[must_use]
    pub fn new(chat: &Chat) -> Self {
        Self {
            id: id_text(chat.id()),
            is_open_chat: chat.is_open_chat().unwrap_or_default(),
            link_id: id_text(chat.link_id().unwrap_or_default()),
            chat: chat.clone(),
        }
    }
}

#[rquickjs::methods]
impl ScriptChat {
    async fn write(&self, text: String) -> rquickjs::Result<String> {
        self.chat
            .write(&text)
            .await
            .map(|log| id_text(log.log_id))
            .map_err(failed)
    }

    /// 이식하면서 빠졌던 것들 — 편집이 없으면 살아 움직이는 메시지를 못 만든다.
    async fn edit(&self, log_id: String, text: String) -> rquickjs::Result<()> {
        let log = self.fetch(&log_id).await?;
        self.chat.edit(&log, &text).await.map(drop).map_err(failed)
    }

    #[qjs(rename = "replyTo")]
    async fn reply_to(&self, log_id: String, text: String) -> rquickjs::Result<String> {
        let log = self.fetch(&log_id).await?;
        self.chat
            .reply(&log, &text)
            .await
            .map(|log| id_text(log.log_id))
            .map_err(failed)
    }

    async fn forward(&self, log_id: String) -> rquickjs::Result<()> {
        let log = self.fetch(&log_id).await?;
        self.chat.forward(&log).await.map(drop).map_err(failed)
    }

    #[qjs(rename = "logText")]
    async fn log_text(&self, log_id: String) -> rquickjs::Result<String> {
        Ok(self.fetch(&log_id).await?.message)
    }

    async fn member(&self, user_id: String) -> rquickjs::Result<Option<ScriptAuthor>> {
        Ok(self
            .chat
            .member(id_of(&user_id)?)
            .await
            .map_err(failed)?
            .map(ScriptAuthor::new))
    }

    fn author(&self, user_id: String) -> rquickjs::Result<ScriptAuthor> {
        Ok(ScriptAuthor::new(self.chat.author(id_of(&user_id)?)))
    }

    #[qjs(rename = "knownMembers")]
    fn known_members(&self) -> Vec<ScriptAuthor> {
        self.chat
            .known_members()
            .into_iter()
            .map(ScriptAuthor::new)
            .collect()
    }

    async fn invite(&self, user_ids: Vec<String>) -> rquickjs::Result<()> {
        let user_ids = user_ids
            .iter()
            .map(|id| id_of(id))
            .collect::<rquickjs::Result<Vec<_>>>()?;
        self.chat.invite(user_ids).await.map_err(failed)
    }

    #[qjs(rename = "setPushAlert")]
    async fn set_push_alert(&self, on: bool) -> rquickjs::Result<()> {
        self.chat.set_push_alert(on).await.map(drop).map_err(failed)
    }

    async fn photo(
        &self,
        bytes: Vec<u8>,
        width: i32,
        height: i32,
        name: String,
    ) -> rquickjs::Result<()> {
        self.chat
            .send_photo(&bytes, width, height, &name)
            .await
            .map(drop)
            .map_err(failed)
    }

    async fn video(
        &self,
        bytes: Vec<u8>,
        width: i32,
        height: i32,
        name: String,
    ) -> rquickjs::Result<()> {
        self.chat
            .send_video(&bytes, width, height, &name)
            .await
            .map(drop)
            .map_err(failed)
    }

    async fn delete(&self, log_id: String) -> rquickjs::Result<bool> {
        self.chat
            .delete(id_of(&log_id)?)
            .await
            .map(|log| log.is_some())
            .map_err(failed)
    }

    async fn hide(&self, log_id: String) -> rquickjs::Result<String> {
        self.chat.hide(&[id_of(&log_id)?]).await.map_err(failed)
    }

    async fn react(&self, log_id: String, reaction: i32) -> rquickjs::Result<()> {
        let reaction = ReactionType::new(reaction)
            .ok_or_else(|| failed(format!("{reaction} 은 없는 반응이다")))?;
        self.chat
            .react(id_of(&log_id)?, reaction)
            .await
            .map_err(failed)
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
    async fn write_in_thread(&self, thread_id: i64, text: String) -> rquickjs::Result<String> {
        self.chat
            .write_in_thread(thread_id, &text)
            .await
            .map(|log| id_text(log.log_id))
            .map_err(failed)
    }

    #[qjs(rename = "cancelReaction")]
    async fn cancel_reaction(&self, log_id: String) -> rquickjs::Result<()> {
        self.chat
            .cancel_reaction(id_of(&log_id)?)
            .await
            .map_err(failed)
    }

    #[qjs(rename = "kickLeave")]
    async fn kick_leave(&self) -> rquickjs::Result<String> {
        self.chat.kick_leave().await.map(id_text).map_err(failed)
    }

    #[qjs(rename = "toString")]
    fn to_string_js(&self) -> String {
        format!("Chat({})", self.id)
    }
}
