use std::path::{Path, PathBuf};

use kakao_loco_client::prelude::*;

use crate::api::ScriptMessage;
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

impl Handler for ScriptHost {
    async fn on_message(&mut self, message: Message<'_>) -> Result<(), ClientError> {
        let carried = ScriptMessage::new(&message);
        for (name, script) in &self.scripts {
            if !script.handles_messages() {
                continue;
            }
            if let Err(error) = script.on_message(carried.clone()).await {
                tracing::error!(script = %name, %error, "script failed");
            }
        }
        Ok(())
    }
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
