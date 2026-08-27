use std::sync::Arc;

use kakao_loco_client::prelude::*;
use rquickjs::JsLifetime;
use rquickjs::class::Trace;

use super::{ScriptChat, failed};

#[derive(Clone, Trace, JsLifetime)]
#[rquickjs::class(rename = "Link")]
pub struct ScriptLink {
    #[qjs(get)]
    pub link_id: i64,
    #[qjs(get)]
    pub name: String,
    #[qjs(get)]
    pub url: String,
    #[qjs(get)]
    pub description: String,
    #[qjs(get)]
    pub member_count: i32,
    #[qjs(get)]
    pub member_limit: i32,
    #[qjs(get)]
    pub needs_passcode: bool,
    #[qjs(get)]
    pub active: bool,
    #[qjs(skip_trace)]
    link: Arc<Link>,
}

impl ScriptLink {
    #[must_use]
    pub fn new(link: Link) -> Self {
        Self {
            link_id: link.link_id,
            name: link.name.clone(),
            url: link.url.clone(),
            description: link.description.clone(),
            member_count: link.member_count,
            member_limit: link.member_limit,
            needs_passcode: link.needs_passcode,
            active: link.active,
            link: Arc::new(link),
        }
    }
}

#[rquickjs::methods]
impl ScriptLink {
    async fn join(&self) -> rquickjs::Result<ScriptChat> {
        self.link
            .join(JoinProfile::Main)
            .await
            .map(|joined| ScriptChat::new(&joined.chat))
            .map_err(failed)
    }

    #[qjs(rename = "joinAs")]
    async fn join_as(&self, nickname: String) -> rquickjs::Result<ScriptChat> {
        self.link
            .join(JoinProfile::nickname(nickname))
            .await
            .map(|joined| ScriptChat::new(&joined.chat))
            .map_err(failed)
    }

    #[qjs(rename = "joinWithPasscode")]
    async fn join_with_passcode(&self, passcode: String) -> rquickjs::Result<ScriptChat> {
        self.link
            .join_with_passcode(JoinProfile::Main, &passcode)
            .await
            .map(|joined| ScriptChat::new(&joined.chat))
            .map_err(failed)
    }

    #[qjs(rename = "toString")]
    fn to_string_js(&self) -> String {
        format!("Link({}, {})", self.link_id, self.name)
    }
}
