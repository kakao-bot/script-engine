use kakao_loco_client::Room;
use rquickjs::JsLifetime;
use rquickjs::class::Trace;

#[derive(Clone, Trace, JsLifetime)]
#[rquickjs::class(rename = "Room")]
pub struct ScriptRoom {
    #[qjs(get)]
    pub chat_id: i64,
    #[qjs(get)]
    pub kind: String,
    #[qjs(get)]
    pub member_count: i32,
    #[qjs(get)]
    pub unread: i32,
    #[qjs(get)]
    pub last_log_id: i64,
    #[qjs(get)]
    pub link_id: i64,
    #[qjs(get)]
    pub is_full: bool,
}

impl ScriptRoom {
    #[must_use]
    pub fn new(room: &Room) -> Self {
        Self {
            chat_id: room.chat_id,
            kind: room.kind.clone(),
            member_count: room.member_count,
            unread: room.unread,
            last_log_id: room.last_log_id.unwrap_or_default(),
            link_id: room.link_id,
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
