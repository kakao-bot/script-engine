use kakao_loco_client::prelude::*;
use rune::alloc::fmt::TryWrite;
use rune::runtime::{Formatter, Ref, VmResult};
use rune::vm_write;

use super::{ScriptChat, ScriptLink, ScriptRoom, text_of};

#[derive(Clone, rune::Any)]
#[rune(item = ::bot)]
pub struct ScriptSession {
    #[rune(get)]
    pub user_id: i64,
    pub(crate) session: Session,
}

impl ScriptSession {
    #[must_use]
    pub fn new(session: Session) -> Self {
        Self {
            user_id: session.user_id(),
            session,
        }
    }

    #[rune::function(keep, instance, protocol = DEBUG_FMT)]
    fn debug_fmt(&self, f: &mut Formatter) -> VmResult<()> {
        vm_write!(f, "ScriptSession {{ user_id: {} }}", self.user_id)
    }
}

#[rune::function(instance, keep)]
fn chat(this: Ref<ScriptSession>, chat_id: i64) -> ScriptChat {
    ScriptChat::new(&this.session.chat(chat_id))
}

#[rune::function(instance, keep)]
async fn chats(this: Ref<ScriptSession>) -> Result<Vec<ScriptRoom>, String> {
    Ok(this
        .session
        .chats()
        .await
        .map_err(text_of)?
        .iter()
        .map(ScriptRoom::new)
        .collect())
}

#[rune::function(instance, keep)]
async fn create(this: Ref<ScriptSession>, member_ids: Vec<i64>) -> Result<ScriptChat, String> {
    this.session
        .create(member_ids)
        .await
        .map(|chat| ScriptChat::new(&chat))
        .map_err(text_of)
}

#[rune::function(instance, keep)]
async fn create_memo(this: Ref<ScriptSession>) -> Result<ScriptChat, String> {
    this.session
        .create_memo()
        .await
        .map(|chat| ScriptChat::new(&chat))
        .map_err(text_of)
}

#[rune::function(instance, keep)]
async fn open_link(this: Ref<ScriptSession>, url: String) -> Result<Option<ScriptLink>, String> {
    Ok(this
        .session
        .open_link(&url)
        .await
        .map_err(text_of)?
        .map(ScriptLink::new))
}

#[rune::function(instance, keep)]
async fn open_link_by_id(
    this: Ref<ScriptSession>,
    link_id: i64,
) -> Result<Option<ScriptLink>, String> {
    Ok(this
        .session
        .open_link_by_id(link_id)
        .await
        .map_err(text_of)?
        .map(ScriptLink::new))
}

#[rune::function(instance, keep)]
fn is_closed(this: Ref<ScriptSession>) -> bool {
    this.session.is_closed()
}

pub fn install(module: &mut rune::Module) -> Result<(), rune::ContextError> {
    module.ty::<ScriptSession>()?;
    module.function_meta(chat__meta)?;
    module.function_meta(chats__meta)?;
    module.function_meta(create__meta)?;
    module.function_meta(create_memo__meta)?;
    module.function_meta(open_link__meta)?;
    module.function_meta(open_link_by_id__meta)?;
    module.function_meta(is_closed__meta)?;
    module.function_meta(ScriptSession::debug_fmt__meta)?;
    Ok(())
}
