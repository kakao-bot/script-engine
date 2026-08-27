use kakao_loco_client::Room;
use rquickjs::JsLifetime;
use rquickjs::class::Trace;

use super::id_text;

#[derive(Clone, Trace, JsLifetime)]
#[rquickjs::class(rename = "Room")]
pub struct ScriptRoom {
    #[qjs(get)]
    pub chat_id: String,
    #[qjs(get)]
    pub kind: String,
    #[qjs(get)]
    pub member_count: i32,
    #[qjs(get)]
    pub unread: i32,
    #[qjs(get)]
    pub last_log_id: String,
    #[qjs(get)]
    pub link_id: String,
    #[qjs(get)]
    pub is_full: bool,
}

impl ScriptRoom {
    #[must_use]
    pub fn new(room: &Room) -> Self {
        Self {
            chat_id: id_text(room.chat_id),
            kind: room.kind.clone(),
            member_count: room.member_count,
            unread: room.unread,
            last_log_id: id_text(room.last_log_id.unwrap_or_default()),
            link_id: id_text(room.link_id),
            is_full: room.is_full,
        }
    }
}

#[rquickjs::methods]
impl ScriptRoom {
    #[qjs(rename = "toString")]
    fn to_string_js(&self) -> String {
        format!("Room({}, {})", self.chat_id, self.kind)
    }
}
