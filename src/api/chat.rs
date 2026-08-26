use kakao_loco_client::core::command::chat::{ChatLog, SpoilerBuilder};
use kakao_loco_client::core::ids::LogId;
use kakao_loco_client::prelude::*;
use rune::alloc::fmt::TryWrite;
use rune::runtime::{Formatter, Ref, VmResult};
use rune::vm_write;

use super::{ScriptAuthor, ScriptRoom, ScriptSession, text_of};

#[derive(Clone, rune::Any)]
#[rune(item = ::bot)]
pub struct ScriptChat {
    #[rune(get)]
    pub id: i64,
    #[rune(get)]
    pub kind: String,
    #[rune(get)]
    pub is_open_chat: bool,
    #[rune(get)]
    pub link_id: i64,
    pub(crate) chat: Chat,
}

impl ScriptChat {
    #[must_use]
    pub fn new(chat: &Chat) -> Self {
        Self {
            id: chat.id(),
            kind: chat
                .kind()
                .map(|kind| format!("{kind:?}"))
                .unwrap_or_default(),
            is_open_chat: chat.is_open_chat().unwrap_or_default(),
            link_id: chat.link_id().unwrap_or_default(),
            chat: chat.clone(),
        }
    }

    async fn fetch(&self, log_id: LogId) -> Result<ChatLog, String> {
        self.chat
            .log(log_id)
            .await
            .map_err(text_of)?
            .ok_or_else(|| format!("로그 {log_id} 이 없다"))
    }

    #[rune::function(keep, instance, protocol = DEBUG_FMT)]
    fn debug_fmt(&self, f: &mut Formatter) -> VmResult<()> {
        vm_write!(
            f,
            "ScriptChat {{ id: {}, kind: {:?}, is_open_chat: {}, link_id: {} }}",
            self.id,
            self.kind,
            self.is_open_chat,
            self.link_id
        )
    }
}

#[rune::function(instance, keep)]
async fn write(this: Ref<ScriptChat>, text: String) -> Result<i64, String> {
    this.chat
        .write(&text)
        .await
        .map(|log| log.log_id)
        .map_err(text_of)
}

#[rune::function(instance, keep)]
async fn write_in_thread(
    this: Ref<ScriptChat>,
    thread_id: i64,
    text: String,
) -> Result<i64, String> {
    this.chat
        .write_in_thread(thread_id, &text)
        .await
        .map(|log| log.log_id)
        .map_err(text_of)
}

#[rune::function(instance, keep)]
async fn write_spoiling(
    this: Ref<ScriptChat>,
    shown: String,
    hidden: String,
) -> Result<i64, String> {
    let (text, spoilers) = SpoilerBuilder::new().text(&shown).hidden(&hidden).build();
    this.chat
        .write_spoiling(&text, &spoilers)
        .await
        .map(|log| log.log_id)
        .map_err(text_of)
}

#[rune::function(instance, keep)]
async fn reply_to(this: Ref<ScriptChat>, log_id: LogId, text: String) -> Result<i64, String> {
    let log = this.fetch(log_id).await?;
    this.chat
        .reply(&log, &text)
        .await
        .map(|log| log.log_id)
        .map_err(text_of)
}

#[rune::function(instance, keep)]
async fn edit(this: Ref<ScriptChat>, log_id: LogId, text: String) -> Result<(), String> {
    let log = this.fetch(log_id).await?;
    this.chat.edit(&log, &text).await.map(drop).map_err(text_of)
}

#[rune::function(instance, keep)]
async fn delete(this: Ref<ScriptChat>, log_id: LogId) -> Result<bool, String> {
    this.chat
        .delete(log_id)
        .await
        .map(|log| log.is_some())
        .map_err(text_of)
}

#[rune::function(instance, keep)]
async fn forward(this: Ref<ScriptChat>, log_id: LogId) -> Result<(), String> {
    let log = this.fetch(log_id).await?;
    this.chat.forward(&log).await.map(drop).map_err(text_of)
}

#[rune::function(instance, keep)]
async fn hide(this: Ref<ScriptChat>, log_id: LogId) -> Result<String, String> {
    this.chat.hide(&[log_id]).await.map_err(text_of)
}

#[rune::function(instance, keep)]
async fn log_text(this: Ref<ScriptChat>, log_id: LogId) -> Result<String, String> {
    Ok(this.fetch(log_id).await?.message)
}

#[rune::function(instance, keep)]
async fn members(this: Ref<ScriptChat>) -> Result<Vec<ScriptAuthor>, String> {
    Ok(this
        .chat
        .members()
        .await
        .map_err(text_of)?
        .into_iter()
        .map(ScriptAuthor::new)
        .collect())
}

#[rune::function(instance, keep)]
async fn member(this: Ref<ScriptChat>, user_id: i64) -> Result<Option<ScriptAuthor>, String> {
    Ok(this
        .chat
        .member(user_id)
        .await
        .map_err(text_of)?
        .map(ScriptAuthor::new))
}

#[rune::function(instance, keep)]
fn author(this: Ref<ScriptChat>, user_id: i64) -> ScriptAuthor {
    ScriptAuthor::new(this.chat.author(user_id))
}

#[rune::function(instance, keep)]
fn known_members(this: Ref<ScriptChat>) -> Vec<ScriptAuthor> {
    this.chat
        .known_members()
        .into_iter()
        .map(ScriptAuthor::new)
        .collect()
}

#[rune::function(instance, keep)]
async fn open(this: Ref<ScriptChat>, token: LogId) -> Result<ScriptRoom, String> {
    this.chat
        .open(token)
        .await
        .map(|room| ScriptRoom::new(&room))
        .map_err(text_of)
}

#[rune::function(instance, keep)]
async fn history(
    this: Ref<ScriptChat>,
    current: LogId,
    max: LogId,
    count: i32,
) -> Result<Vec<String>, String> {
    Ok(this
        .chat
        .history(current, max, count)
        .await
        .map_err(text_of)?
        .into_iter()
        .map(|log| log.message)
        .collect())
}

#[rune::function(instance, keep)]
async fn invite(this: Ref<ScriptChat>, user_ids: Vec<i64>) -> Result<(), String> {
    this.chat.invite(user_ids).await.map_err(text_of)
}

#[rune::function(instance, keep)]
async fn set_push_alert(this: Ref<ScriptChat>, on: bool) -> Result<(), String> {
    this.chat
        .set_push_alert(on)
        .await
        .map(drop)
        .map_err(text_of)
}

#[rune::function(instance, keep)]
async fn react(this: Ref<ScriptChat>, log_id: LogId, reaction: i32) -> Result<(), String> {
    let reaction =
        ReactionType::new(reaction).ok_or_else(|| format!("{reaction} 은 없는 반응이다"))?;
    this.chat.react(log_id, reaction).await.map_err(text_of)
}

#[rune::function(instance, keep)]
async fn cancel_reaction(this: Ref<ScriptChat>, log_id: LogId) -> Result<(), String> {
    this.chat.cancel_reaction(log_id).await.map_err(text_of)
}

#[rune::function(instance, keep)]
async fn photo(
    this: Ref<ScriptChat>,
    bytes: Vec<u8>,
    width: i32,
    height: i32,
    name: String,
) -> Result<(), String> {
    this.chat
        .send_photo(&bytes, width, height, &name)
        .await
        .map(drop)
        .map_err(text_of)
}

#[rune::function(instance, keep)]
async fn video(
    this: Ref<ScriptChat>,
    bytes: Vec<u8>,
    width: i32,
    height: i32,
    name: String,
) -> Result<(), String> {
    this.chat
        .send_video(&bytes, width, height, &name)
        .await
        .map(drop)
        .map_err(text_of)
}

#[rune::function(instance, keep)]
async fn close(this: Ref<ScriptChat>) -> Result<(), String> {
    this.chat.close().await.map_err(text_of)
}

#[rune::function(instance, keep)]
async fn leave(this: Ref<ScriptChat>) -> Result<(), String> {
    this.chat.leave().await.map_err(text_of)
}

#[rune::function(instance, keep)]
async fn kick_leave(this: Ref<ScriptChat>) -> Result<i64, String> {
    this.chat.kick_leave().await.map_err(text_of)
}

#[rune::function(instance, keep)]
fn session(this: Ref<ScriptChat>) -> ScriptSession {
    ScriptSession::new(this.chat.session().clone())
}

pub fn install(module: &mut rune::Module) -> Result<(), rune::ContextError> {
    module.ty::<ScriptChat>()?;
    module.function_meta(write__meta)?;
    module.function_meta(write_in_thread__meta)?;
    module.function_meta(write_spoiling__meta)?;
    module.function_meta(reply_to__meta)?;
    module.function_meta(edit__meta)?;
    module.function_meta(delete__meta)?;
    module.function_meta(forward__meta)?;
    module.function_meta(hide__meta)?;
    module.function_meta(log_text__meta)?;
    module.function_meta(members__meta)?;
    module.function_meta(member__meta)?;
    module.function_meta(author__meta)?;
    module.function_meta(known_members__meta)?;
    module.function_meta(open__meta)?;
    module.function_meta(history__meta)?;
    module.function_meta(invite__meta)?;
    module.function_meta(set_push_alert__meta)?;
    module.function_meta(react__meta)?;
    module.function_meta(cancel_reaction__meta)?;
    module.function_meta(photo__meta)?;
    module.function_meta(video__meta)?;
    module.function_meta(close__meta)?;
    module.function_meta(leave__meta)?;
    module.function_meta(kick_leave__meta)?;
    module.function_meta(session__meta)?;
    module.function_meta(ScriptChat::debug_fmt__meta)?;
    Ok(())
}
