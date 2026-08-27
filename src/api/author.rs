use kakao_loco_client::api::author::Author;
use rquickjs::JsLifetime;
use rquickjs::class::Trace;

use super::{failed, id_text};

#[derive(Clone, Trace, JsLifetime)]
#[rquickjs::class(rename = "Author")]
pub struct ScriptAuthor {
    #[qjs(get)]
    pub id: String,
    #[qjs(get)]
    pub name: String,
    #[qjs(get)]
    pub is_me: bool,
    #[qjs(get)]
    pub profile_url: String,
    #[qjs(get)]
    pub member_type: i32,
    #[qjs(skip_trace)]
    author: Author,
}

impl ScriptAuthor {
    #[must_use]
    pub fn new(author: Author) -> Self {
        Self {
            id: id_text(author.id()),
            name: author.display(),
            is_me: author.is_me(),
            profile_url: author.profile_url().unwrap_or_default().to_owned(),
            member_type: author.member_type().unwrap_or_default(),
            author,
        }
    }
}

#[rquickjs::methods]
impl ScriptAuthor {
    async fn kick(&self) -> rquickjs::Result<()> {
        self.author.kick().await.map(drop).map_err(failed)
    }

    async fn blind(&self) -> rquickjs::Result<String> {
        self.author.blind().await.map(id_text).map_err(failed)
    }

    #[qjs(rename = "toString")]
    fn to_string_js(&self) -> String {
        format!("{} ({})", self.name, self.id)
    }
}
