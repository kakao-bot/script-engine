use kakao_loco_client::prelude::*;
use rquickjs::JsLifetime;
use rquickjs::class::Trace;

use super::{ScriptChat, ScriptLink, ScriptRoom, failed, id_of, id_text};

#[derive(Clone, Trace, JsLifetime)]
#[rquickjs::class(rename = "Session")]
pub struct ScriptSession {
    #[qjs(get)]
    pub user_id: String,
    #[qjs(skip_trace)]
    session: Session,
}

impl ScriptSession {
    #[must_use]
    pub fn new(session: Session) -> Self {
        Self {
            user_id: id_text(session.user_id()),
            session,
        }
    }
}

#[rquickjs::methods]
impl ScriptSession {
    fn chat(&self, chat_id: String) -> rquickjs::Result<ScriptChat> {
        Ok(ScriptChat::new(&self.session.chat(id_of(&chat_id)?)))
    }

    async fn chats(&self) -> rquickjs::Result<Vec<ScriptRoom>> {
        Ok(self
            .session
            .chats()
            .await
            .map_err(failed)?
            .iter()
            .map(ScriptRoom::new)
            .collect())
    }

    async fn create(&self, member_ids: Vec<String>) -> rquickjs::Result<ScriptChat> {
        let member_ids = member_ids
            .iter()
            .map(|id| id_of(id))
            .collect::<rquickjs::Result<Vec<_>>>()?;
        self.session
            .create(member_ids)
            .await
            .map(|chat| ScriptChat::new(&chat))
            .map_err(failed)
    }

    #[qjs(rename = "createMemo")]
    async fn create_memo(&self) -> rquickjs::Result<ScriptChat> {
        self.session
            .create_memo()
            .await
            .map(|chat| ScriptChat::new(&chat))
            .map_err(failed)
    }

    #[qjs(rename = "openLink")]
    async fn open_link(&self, url: String) -> rquickjs::Result<Option<ScriptLink>> {
        Ok(self
            .session
            .open_link(&url)
            .await
            .map_err(failed)?
            .map(ScriptLink::new))
    }

    #[qjs(rename = "openLinkById")]
    async fn open_link_by_id(&self, link_id: String) -> rquickjs::Result<Option<ScriptLink>> {
        Ok(self
            .session
            .open_link_by_id(id_of(&link_id)?)
            .await
            .map_err(failed)?
            .map(ScriptLink::new))
    }

    #[qjs(rename = "toString")]
    fn to_string_js(&self) -> String {
        format!("Session({})", self.user_id)
    }
}
