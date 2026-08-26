pub mod author;
pub mod chat;
pub mod link;
pub mod message;
pub mod room;
pub mod session;

pub use author::ScriptAuthor;
pub use chat::ScriptChat;
pub use link::{ScriptJoined, ScriptLink};
pub use message::ScriptMessage;
pub use room::ScriptRoom;
pub use session::ScriptSession;

pub(crate) fn text_of(error: impl std::fmt::Display) -> String {
    error.to_string()
}

macro_rules! try_clone {
    ($($ty:ty),* $(,)?) => {
        $(impl rune::alloc::clone::TryClone for $ty {
            fn try_clone(&self) -> Result<Self, rune::alloc::Error> {
                Ok(self.clone())
            }
        })*
    };
}

try_clone!(
    ScriptAuthor,
    ScriptChat,
    ScriptJoined,
    ScriptLink,
    ScriptRoom,
    ScriptSession
);

/// Named so a script never has to remember which number is which reaction.
const REACTIONS: [(&str, i32); 7] = [
    ("CANCEL", 0),
    ("HEART", 1),
    ("LIKE", 2),
    ("CHECK", 3),
    ("LAUGH", 4),
    ("SURPRISE", 5),
    ("SAD", 6),
];

pub fn module() -> Result<rune::Module, rune::ContextError> {
    let mut module = rune::Module::with_crate("bot")?;
    for (name, value) in REACTIONS {
        module.constant(name, value).build()?;
    }
    message::install(&mut module)?;
    chat::install(&mut module)?;
    session::install(&mut module)?;
    author::install(&mut module)?;
    room::install(&mut module)?;
    link::install(&mut module)?;
    Ok(module)
}
