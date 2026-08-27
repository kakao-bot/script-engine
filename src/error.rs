use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("{name}\n{report}")]
    Compile { name: String, report: String },
    #[error("{0}")]
    Vm(String),
    #[error("{0}")]
    Rune(String),
    #[error("eval 안에서 다시 eval 할 수 없다")]
    Nested,
    #[error("배경 스크립트가 이미 {limit}개다")]
    Crowded { limit: usize },
    #[error("{path}: {source}")]
    Unreadable {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

impl ScriptError {
    #[must_use]
    pub fn is_compile(&self) -> bool {
        matches!(self, Self::Compile { .. })
    }
}
