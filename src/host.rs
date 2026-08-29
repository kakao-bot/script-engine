use std::path::Path;

use kakao_loco_client::api::author::Author;
use kakao_loco_client::core::command::chat::{
    ChgLogMeta, DecUnread, Feed, FeedPush, Left, MemberChange,
};
use kakao_loco_client::core::command::open_link::SyncLinkProfile;
use kakao_loco_client::network::hops::checkin::Plan;
use kakao_loco_client::network::transport::Endpoint;
use kakao_loco_client::prelude::*;

use crate::api::{ScriptAuthor, ScriptChat, ScriptMessage};
use crate::engine::Script;
use crate::error::ScriptError;

pub const ENTRY: &str = "index.js";

pub struct ScriptHost {
    scripts: Vec<Script>,
}

impl ScriptHost {
    #[must_use]
    pub fn new() -> Self {
        Self {
            scripts: Vec::new(),
        }
    }

    pub async fn load_dir(directory: &Path) -> Result<Self, ScriptError> {
        let entry = directory.join(ENTRY);
        let code = std::fs::read_to_string(&entry).map_err(|source| ScriptError::Unreadable {
            path: entry.display().to_string(),
            source,
        })?;

        let mut host = Self::new();
        host.scripts
            .push(Script::compile_in(ENTRY, &code, directory).await?);
        Ok(host)
    }

    pub async fn add(&mut self, name: &str, code: &str) -> Result<(), ScriptError> {
        self.scripts.push(Script::compile(name, code).await?);
        Ok(())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.scripts.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty()
    }

    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.scripts.iter().map(Script::name).collect()
    }

    #[must_use]
    pub fn drivers(&self) -> Vec<rquickjs::runtime::DriveFuture> {
        self.scripts.iter().map(Script::drive).collect()
    }
}

impl Default for ScriptHost {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptHost {
    async fn fire<A>(&self, hook: &'static str, args: A)
    where
        A: for<'js> rquickjs::function::IntoArgs<'js> + Send + Clone + 'static,
    {
        tracing::debug!(hook, "event");
        for script in &self.scripts {
            if let Err(error) = script.call(hook, args.clone()).await {
                tracing::error!(script = %script.name(), hook, %error, "script failed");
            }
        }
    }
}

impl Handler for ScriptHost {
    async fn on_message(&mut self, message: Message<'_>) -> Result<(), ClientError> {
        self.fire("onMessage", (ScriptMessage::new(&message),))
            .await;
        Ok(())
    }

    async fn on_join(&mut self, chat: Chat, members: Vec<Author>) -> Result<(), ClientError> {
        self.fire("onJoin", (ScriptChat::new(&chat), authors(members)))
            .await;
        Ok(())
    }

    async fn on_leave(&mut self, chat: Chat, members: Vec<Author>) -> Result<(), ClientError> {
        self.fire("onLeave", (ScriptChat::new(&chat), authors(members)))
            .await;
        Ok(())
    }

    async fn on_read(&mut self, chat: Chat, read: &DecUnread) -> Result<(), ClientError> {
        self.fire(
            "onRead",
            (ScriptChat::new(&chat), read.user_id, read.watermark),
        )
        .await;
        Ok(())
    }

    async fn on_log_meta(&mut self, chat: Chat, changed: &ChgLogMeta) -> Result<(), ClientError> {
        self.fire(
            "onReaction",
            (
                ScriptChat::new(&chat),
                changed.log_id,
                changed.meta_type,
                changed.content.clone(),
            ),
        )
        .await;
        Ok(())
    }

    async fn on_member_change(
        &mut self,
        chat: Chat,
        change: &MemberChange,
        members: Vec<Author>,
    ) -> Result<(), ClientError> {
        self.fire(
            "onMemberChange",
            (ScriptChat::new(&chat), change.joined, authors(members)),
        )
        .await;
        Ok(())
    }

    async fn on_feed(&mut self, chat: Chat, feed: &Feed) -> Result<(), ClientError> {
        self.fire("onFeed", (ScriptChat::new(&chat), feed.feed_type.value()))
            .await;
        Ok(())
    }

    async fn on_sync_join(&mut self, chat: Chat, _push: &FeedPush) -> Result<(), ClientError> {
        self.fire("onSyncJoin", (ScriptChat::new(&chat),)).await;
        Ok(())
    }

    async fn on_link_profile(
        &mut self,
        chat: Chat,
        changed: &SyncLinkProfile,
    ) -> Result<(), ClientError> {
        self.fire("onLinkProfile", (ScriptChat::new(&chat), changed.link_id))
            .await;
        Ok(())
    }

    async fn on_left(&mut self, chat: Chat, _left: &Left) -> Result<(), ClientError> {
        self.fire("onLeft", (ScriptChat::new(&chat),)).await;
        Ok(())
    }

    async fn on_event(
        &mut self,
        _session: &Session,
        event: &SessionEvent,
    ) -> Result<(), ClientError> {
        match event {
            SessionEvent::LoggedIn { response, .. } => {
                self.fire("onLogin", (response.user_id.unwrap_or_default(),))
                    .await;
            }
            SessionEvent::Listening { ping_interval } => {
                let seconds = i64::try_from(ping_interval.as_secs()).unwrap_or_default();
                self.fire("onListening", (seconds,)).await;
            }
            SessionEvent::Push { packet, kind, .. } => {
                let method = packet.header.method.to_string();
                match kind {
                    PushKind::KickedOut(_) => self.fire("onKicked", ()).await,
                    PushKind::ChangeServer | PushKind::Restarted(_) => {
                        self.fire("onMoved", (method,)).await;
                    }
                    PushKind::MetaChanged(changed) => {
                        crate::api::chat::forget_title(changed.chat_id);
                        let meta = changed.meta.as_ref();
                        self.fire(
                            "onMetaChange",
                            (
                                changed.chat_id,
                                meta.map_or(0, |meta| meta.meta_type),
                                meta.map(|meta| meta.content.clone()).unwrap_or_default(),
                            ),
                        )
                        .await;
                    }
                    _ => self.fire("onPush", (method,)).await,
                }
            }
            SessionEvent::PingAcknowledged => {}
        }
        Ok(())
    }

    async fn on_connect(&mut self, _plan: &Plan, _endpoint: &Endpoint) -> Result<(), ClientError> {
        self.fire("onConnect", ()).await;
        Ok(())
    }

    async fn on_close(&mut self, outcome: &Result<(), ClientError>) {
        let reason = match outcome {
            Ok(()) => String::new(),
            Err(error) => error.to_string(),
        };
        self.fire("onClose", (reason,)).await;
    }
}

fn authors(members: Vec<Author>) -> Vec<ScriptAuthor> {
    members.into_iter().map(ScriptAuthor::new).collect()
}

#[cfg(test)]
mod tests {
    use super::ScriptHost;

    #[test]
    fn a_host_starts_with_nothing_loaded() {
        assert!(ScriptHost::new().is_empty());
    }

    #[tokio::test]
    async fn a_loaded_script_is_listed_by_name() {
        let mut host = ScriptHost::new();

        host.add("index.js", "globalThis.onMessage = async () => {};")
            .await
            .unwrap();

        assert_eq!(host.len(), 1);
        assert_eq!(host.names(), vec!["index.js"]);
    }

    #[tokio::test]
    async fn an_entry_and_what_it_imports_load_as_one_script() {
        let dir = std::path::PathBuf::from("target/fixtures/host");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::write(dir.join("lib/dice.js"), "export const roll = () => 4;").unwrap();
        std::fs::write(
            dir.join("index.js"),
            "import { roll } from './lib/dice.js';\nglobalThis.onMessage = async () => { if (roll() !== 4) throw 'bad'; };",
        )
        .unwrap();

        let host = ScriptHost::load_dir(&dir).await.unwrap();

        assert_eq!(host.len(), 1, "one entry, whatever it imports");
        assert_eq!(host.names(), vec!["index.js"]);
    }

    #[tokio::test]
    async fn a_directory_without_an_entry_says_which_file_it_wanted() {
        let refused = ScriptHost::load_dir(std::path::Path::new("src")).await;

        let Err(error) = refused else {
            panic!("a directory with no index.js loaded");
        };
        assert!(error.to_string().contains("index.js"), "{error}");
    }

    #[tokio::test]
    async fn a_broken_script_names_itself_in_the_complaint() {
        let mut host = ScriptHost::new();

        let refused = host
            .add("bad.js", "globalThis.onMessage = async ( => {};")
            .await;

        let Err(error) = refused else {
            panic!("a broken script loaded");
        };
        assert!(error.is_compile(), "{error}");
        assert!(error.to_string().starts_with("bad.js"));
        assert!(host.is_empty());
    }
}
