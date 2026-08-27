use std::path::{Path, PathBuf};

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

pub const EXTENSION: &str = "rn";

pub struct ScriptHost {
    scripts: Vec<(String, Script)>,
}

impl ScriptHost {
    #[must_use]
    pub fn new() -> Self {
        Self {
            scripts: Vec::new(),
        }
    }

    pub fn load_dir(directory: &Path) -> Result<Self, ScriptError> {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(directory)
            .map_err(|source| ScriptError::Unreadable {
                path: directory.display().to_string(),
                source,
            })?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|kind| kind == EXTENSION))
            .collect();
        paths.sort();

        let mut host = Self::new();
        for path in paths {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let code =
                std::fs::read_to_string(&path).map_err(|source| ScriptError::Unreadable {
                    path: path.display().to_string(),
                    source,
                })?;
            host.add(&name, &code)?;
        }
        Ok(host)
    }

    pub fn add(&mut self, name: &str, code: &str) -> Result<(), ScriptError> {
        self.scripts
            .push((name.to_owned(), Script::compile(name, code)?));
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
        self.scripts.iter().map(|(name, _)| name.as_str()).collect()
    }
}

impl Default for ScriptHost {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptHost {
    async fn fire(&self, hook: &'static str, args: impl rune::runtime::GuardedArgs + Send + Clone) {
        tracing::debug!(hook, "event");
        for (name, script) in &self.scripts {
            if let Err(error) = script.call(hook, args.clone()).await {
                tracing::error!(script = %name, hook, %error, "script failed");
            }
        }
    }
}

impl Handler for ScriptHost {
    async fn on_message(&mut self, message: Message<'_>) -> Result<(), ClientError> {
        self.fire("on_message", (ScriptMessage::new(&message),))
            .await;
        Ok(())
    }

    async fn on_join(&mut self, chat: Chat, members: Vec<Author>) -> Result<(), ClientError> {
        self.fire("on_join", (ScriptChat::new(&chat), authors(members)))
            .await;
        Ok(())
    }

    async fn on_leave(&mut self, chat: Chat, members: Vec<Author>) -> Result<(), ClientError> {
        self.fire("on_leave", (ScriptChat::new(&chat), authors(members)))
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
            "on_member_change",
            (ScriptChat::new(&chat), change.joined, authors(members)),
        )
        .await;
        Ok(())
    }

    async fn on_sync_join(&mut self, chat: Chat, _push: &FeedPush) -> Result<(), ClientError> {
        self.fire("on_sync_join", (ScriptChat::new(&chat),)).await;
        Ok(())
    }

    async fn on_link_profile(
        &mut self,
        chat: Chat,
        changed: &SyncLinkProfile,
    ) -> Result<(), ClientError> {
        self.fire("on_link_profile", (ScriptChat::new(&chat), changed.link_id))
            .await;
        Ok(())
    }

    async fn on_left(&mut self, chat: Chat, _left: &Left) -> Result<(), ClientError> {
        self.fire("on_left", (ScriptChat::new(&chat),)).await;
        Ok(())
    }

    async fn on_read(&mut self, chat: Chat, read: &DecUnread) -> Result<(), ClientError> {
        self.fire(
            "on_read",
            (ScriptChat::new(&chat), read.user_id, read.watermark),
        )
        .await;
        Ok(())
    }

    async fn on_log_meta(&mut self, chat: Chat, changed: &ChgLogMeta) -> Result<(), ClientError> {
        self.fire(
            "on_reaction",
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

    async fn on_feed(&mut self, chat: Chat, feed: &Feed) -> Result<(), ClientError> {
        self.fire("on_feed", (ScriptChat::new(&chat), feed.feed_type.value()))
            .await;
        Ok(())
    }

    async fn on_event(
        &mut self,
        _session: &Session,
        event: &SessionEvent,
    ) -> Result<(), ClientError> {
        match event {
            SessionEvent::LoggedIn { response, .. } => {
                self.fire("on_login", (response.user_id.unwrap_or_default(),))
                    .await;
            }
            SessionEvent::Listening { ping_interval } => {
                let seconds = i64::try_from(ping_interval.as_secs()).unwrap_or_default();
                self.fire("on_listening", (seconds,)).await;
            }
            SessionEvent::Push { packet, kind, .. } => {
                let method = packet.header.method.to_string();
                match kind {
                    PushKind::KickedOut(_) => self.fire("on_kicked", ()).await,
                    PushKind::ChangeServer | PushKind::Restarted(_) => {
                        self.fire("on_moved", (method,)).await;
                    }
                    PushKind::MetaChanged(changed) => {
                        let meta = changed.meta.as_ref();
                        self.fire(
                            "on_meta_change",
                            (
                                changed.chat_id,
                                meta.map_or(0, |meta| meta.meta_type),
                                meta.map(|meta| meta.content.clone()).unwrap_or_default(),
                            ),
                        )
                        .await;
                    }
                    _ => self.fire("on_push", (method,)).await,
                }
            }
            SessionEvent::PingAcknowledged => {}
        }
        Ok(())
    }

    async fn on_connect(&mut self, _plan: &Plan, _endpoint: &Endpoint) -> Result<(), ClientError> {
        self.fire("on_connect", ()).await;
        Ok(())
    }

    async fn on_close(&mut self, outcome: &Result<(), ClientError>) {
        let reason = match outcome {
            Ok(()) => String::new(),
            Err(error) => error.to_string(),
        };
        self.fire("on_close", (reason,)).await;
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

    #[test]
    fn a_loaded_script_is_listed_by_name() {
        let mut host = ScriptHost::new();

        host.add("hello.rn", "pub async fn on_message(msg) {}")
            .unwrap();

        assert_eq!(host.len(), 1);
        assert_eq!(host.names(), vec!["hello.rn"]);
    }

    #[test]
    fn the_shipped_script_compiles() {
        let mut host = ScriptHost::new();
        let code = std::fs::read_to_string("scripts/hello.rn").unwrap();

        host.add("hello.rn", &code).unwrap();

        assert_eq!(host.len(), 1);
    }

    #[test]
    fn a_broken_script_names_itself_in_the_complaint() {
        let mut host = ScriptHost::new();

        let refused = host.add("bad.rn", "pub async fn on_message(msg) { msg. }");

        let Err(error) = refused else {
            panic!("a broken script loaded");
        };
        assert!(error.is_compile(), "{error}");
        assert!(error.to_string().starts_with("bad.rn"));
        assert!(host.is_empty());
    }
}
