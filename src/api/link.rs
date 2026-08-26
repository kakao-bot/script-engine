use std::sync::Arc;

use kakao_loco_client::prelude::*;
use rune::alloc::fmt::TryWrite;
use rune::runtime::{Formatter, Ref, VmResult};
use rune::vm_write;

use super::{ScriptChat, text_of};

#[derive(Clone, rune::Any)]
#[rune(item = ::bot)]
pub struct ScriptLink {
    #[rune(get)]
    pub link_id: i64,
    #[rune(get)]
    pub name: String,
    #[rune(get)]
    pub url: String,
    #[rune(get)]
    pub description: String,
    #[rune(get)]
    pub member_count: i32,
    #[rune(get)]
    pub member_limit: i32,
    #[rune(get)]
    pub needs_passcode: bool,
    #[rune(get)]
    pub active: bool,
    pub(crate) link: Arc<Link>,
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

    #[rune::function(keep, instance, protocol = DEBUG_FMT)]
    fn debug_fmt(&self, f: &mut Formatter) -> VmResult<()> {
        vm_write!(
            f,
            "ScriptLink {{ link_id: {}, name: {:?}, members: {}/{}, needs_passcode: {} }}",
            self.link_id,
            self.name,
            self.member_count,
            self.member_limit,
            self.needs_passcode
        )
    }
}

#[derive(Clone, rune::Any)]
#[rune(item = ::bot)]
pub struct ScriptJoined {
    #[rune(get)]
    pub chat: ScriptChat,
    #[rune(get)]
    pub nickname: String,
    #[rune(get)]
    pub user_id: i64,
}

#[rune::function(instance, keep)]
async fn join(this: Ref<ScriptLink>) -> Result<ScriptJoined, String> {
    let joined = this.link.join(JoinProfile::Main).await.map_err(text_of)?;
    Ok(ScriptJoined {
        chat: ScriptChat::new(&joined.chat),
        nickname: joined.nickname,
        user_id: joined.user_id,
    })
}

#[rune::function(instance, keep)]
async fn join_as(this: Ref<ScriptLink>, nickname: String) -> Result<ScriptJoined, String> {
    let joined = this
        .link
        .join(JoinProfile::nickname(nickname))
        .await
        .map_err(text_of)?;
    Ok(ScriptJoined {
        chat: ScriptChat::new(&joined.chat),
        nickname: joined.nickname,
        user_id: joined.user_id,
    })
}

#[rune::function(instance, keep)]
async fn join_with_passcode(
    this: Ref<ScriptLink>,
    passcode: String,
) -> Result<ScriptJoined, String> {
    let joined = this
        .link
        .join_with_passcode(JoinProfile::Main, &passcode)
        .await
        .map_err(text_of)?;
    Ok(ScriptJoined {
        chat: ScriptChat::new(&joined.chat),
        nickname: joined.nickname,
        user_id: joined.user_id,
    })
}

pub fn install(module: &mut rune::Module) -> Result<(), rune::ContextError> {
    module.ty::<ScriptLink>()?;
    module.ty::<ScriptJoined>()?;
    module.function_meta(join__meta)?;
    module.function_meta(join_as__meta)?;
    module.function_meta(join_with_passcode__meta)?;
    module.function_meta(ScriptLink::debug_fmt__meta)?;
    Ok(())
}
