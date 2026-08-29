pub mod author;
pub mod chat;
pub mod fs;
pub mod http;
pub mod link;
pub mod message;
pub mod room;
pub mod session;

pub use author::ScriptAuthor;
pub use chat::ScriptChat;
pub use link::ScriptLink;
pub use message::ScriptMessage;
pub use room::ScriptRoom;
pub use session::ScriptSession;

pub fn id_text(id: i64) -> String {
    id.to_string()
}

pub fn id_of(text: &str) -> rquickjs::Result<i64> {
    text.parse()
        .map_err(|_| failed(format!("{text} 은(는) id가 아니다")))
}

pub(crate) fn failed(error: impl std::fmt::Display) -> rquickjs::Error {
    rquickjs::Error::new_from_js_message("rust", "error", error.to_string())
}
