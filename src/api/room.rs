use kakao_loco_client::Room;
use rune::alloc::fmt::TryWrite;
use rune::runtime::{Formatter, Ref, VmResult};
use rune::vm_write;

#[derive(Clone, rune::Any)]
#[rune(item = ::bot)]
pub struct ScriptRoom {
    pub(crate) display_ids: Vec<i64>,
    #[rune(get)]
    pub chat_id: i64,
    #[rune(get)]
    pub kind: String,
    #[rune(get)]
    pub member_count: i32,
    #[rune(get)]
    pub unread: i32,
    #[rune(get)]
    pub last_log_id: i64,
    #[rune(get)]
    pub link_id: i64,
    #[rune(get)]
    pub is_full: bool,
}

impl ScriptRoom {
    #[must_use]
    pub fn new(room: &Room) -> Self {
        Self {
            display_ids: room.display_user_ids.clone(),
            chat_id: room.chat_id,
            kind: room.kind.clone(),
            member_count: room.member_count,
            unread: room.unread,
            last_log_id: room.last_log_id.unwrap_or_default(),
            link_id: room.link_id,
            is_full: room.is_full,
        }
    }

    #[rune::function(keep, instance, protocol = DEBUG_FMT)]
    fn debug_fmt(&self, f: &mut Formatter) -> VmResult<()> {
        vm_write!(
            f,
            "ScriptRoom {{ chat_id: {}, kind: {:?}, member_count: {}, unread: {}, link_id: {} }}",
            self.chat_id,
            self.kind,
            self.member_count,
            self.unread,
            self.link_id
        )
    }
}

#[rune::function(instance, keep)]
fn display_user_ids(this: Ref<ScriptRoom>) -> Vec<i64> {
    this.display_ids.clone()
}

pub fn install(module: &mut rune::Module) -> Result<(), rune::ContextError> {
    module.ty::<ScriptRoom>()?;
    module.function_meta(display_user_ids__meta)?;
    module.function_meta(ScriptRoom::debug_fmt__meta)?;
    Ok(())
}
