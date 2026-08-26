use kakao_loco_client::api::author::Author;
use kakao_loco_client::core::command::open_link::MemberType;
use rune::alloc::fmt::TryWrite;
use rune::runtime::{Formatter, Ref, VmResult};
use rune::vm_write;

use super::text_of;

#[derive(Clone, rune::Any)]
#[rune(item = ::bot)]
pub struct ScriptAuthor {
    #[rune(get)]
    pub id: i64,
    #[rune(get)]
    pub display: String,
    #[rune(get)]
    pub name: String,
    #[rune(get)]
    pub is_me: bool,
    #[rune(get)]
    pub profile_url: String,
    #[rune(get)]
    pub full_profile_url: String,
    #[rune(get)]
    pub original_profile_url: String,
    #[rune(get)]
    pub member_type: i32,
    pub(crate) author: Author,
}

impl ScriptAuthor {
    #[must_use]
    pub fn new(author: Author) -> Self {
        Self {
            id: author.id(),
            display: author.display(),
            name: author.name().unwrap_or_default().to_owned(),
            is_me: author.is_me(),
            profile_url: author.profile_url().unwrap_or_default().to_owned(),
            full_profile_url: author.full_profile_url().unwrap_or_default().to_owned(),
            original_profile_url: author.original_profile_url().unwrap_or_default().to_owned(),
            member_type: author.member_type().unwrap_or_default(),
            author,
        }
    }

    #[rune::function(keep, instance, protocol = DISPLAY_FMT)]
    fn display_fmt(&self, f: &mut Formatter) -> VmResult<()> {
        vm_write!(f, "{}", self.display)
    }

    #[rune::function(keep, instance, protocol = DEBUG_FMT)]
    fn debug_fmt(&self, f: &mut Formatter) -> VmResult<()> {
        vm_write!(
            f,
            "ScriptAuthor {{ id: {}, display: {:?}, is_me: {}, member_type: {} }}",
            self.id,
            self.display,
            self.is_me,
            self.member_type
        )
    }
}

#[rune::function(instance, keep)]
async fn dm(this: Ref<ScriptAuthor>) -> Result<super::ScriptChat, String> {
    this.author
        .dm()
        .await
        .map(|chat| super::ScriptChat::new(&chat))
        .map_err(text_of)
}

#[rune::function(instance, keep)]
async fn kick(this: Ref<ScriptAuthor>) -> Result<(), String> {
    this.author.kick().await.map(drop).map_err(text_of)
}

#[rune::function(instance, keep)]
async fn kick_and_report(this: Ref<ScriptAuthor>) -> Result<(), String> {
    this.author
        .kick_and_report()
        .await
        .map(drop)
        .map_err(text_of)
}

#[rune::function(instance, keep)]
async fn blind(this: Ref<ScriptAuthor>) -> Result<i64, String> {
    this.author.blind().await.map_err(text_of)
}

#[rune::function(instance, keep)]
async fn set_member_type(this: Ref<ScriptAuthor>, member_type: i32) -> Result<(), String> {
    this.author
        .set_member_type(MemberType::new(member_type))
        .await
        .map(drop)
        .map_err(text_of)
}

#[rune::function(instance, keep)]
fn chat(this: Ref<ScriptAuthor>) -> super::ScriptChat {
    super::ScriptChat::new(&this.author.chat())
}

pub fn install(module: &mut rune::Module) -> Result<(), rune::ContextError> {
    module.ty::<ScriptAuthor>()?;
    module.function_meta(dm__meta)?;
    module.function_meta(kick__meta)?;
    module.function_meta(kick_and_report__meta)?;
    module.function_meta(blind__meta)?;
    module.function_meta(set_member_type__meta)?;
    module.function_meta(chat__meta)?;
    module.function_meta(ScriptAuthor::display_fmt__meta)?;
    module.function_meta(ScriptAuthor::debug_fmt__meta)?;
    Ok(())
}
