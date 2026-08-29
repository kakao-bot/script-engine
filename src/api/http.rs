use rquickjs::JsLifetime;
use rquickjs::class::Trace;
use rquickjs::function::Opt;
use rquickjs::{Ctx, Object};

use super::failed;

#[derive(Clone, Trace, JsLifetime)]
#[rquickjs::class(rename = "Response")]
pub struct ScriptResponse {
    #[qjs(get)]
    pub status: u16,
    #[qjs(get)]
    pub ok: bool,
    #[qjs(get)]
    pub url: String,
    #[qjs(skip_trace)]
    body: String,
}

#[rquickjs::methods]
impl ScriptResponse {
    async fn text(&self) -> String {
        self.body.clone()
    }

    async fn json<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<rquickjs::Value<'js>> {
        ctx.json_parse(self.body.clone())
    }

    #[qjs(rename = "toString")]
    fn to_string_js(&self) -> String {
        format!("Response({}, {})", self.status, self.url)
    }
}

pub fn install<'js>(ctx: &Ctx<'js>) -> rquickjs::Result<()> {
    rquickjs::Class::<ScriptResponse>::define(&ctx.globals())?;
    ctx.globals().set(
        "fetch",
        rquickjs::function::Func::from(|ctx: Ctx<'js>, url: String, options: Opt<Object<'js>>| {
            let request = read_options(&url, options.0);
            let (promise, resolve, reject) = ctx.promise()?;
            ctx.spawn(async move {
                match send(request).await {
                    Ok(response) => {
                        let _ = resolve.call::<_, rquickjs::Value>((response,));
                    }
                    Err(error) => {
                        let _ = reject.call::<_, rquickjs::Value>((error.to_string(),));
                    }
                }
            });
            Ok::<_, rquickjs::Error>(promise)
        }),
    )
}

struct Request {
    url: String,
    method: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

fn read_options(url: &str, options: Option<Object<'_>>) -> Request {
    let Some(options) = options else {
        return Request {
            url: url.to_owned(),
            method: "GET".to_owned(),
            headers: Vec::new(),
            body: None,
        };
    };

    let headers = options
        .get::<_, Object>("headers")
        .map(|headers| {
            headers
                .keys::<String>()
                .filter_map(Result::ok)
                .filter_map(|key| {
                    let value = headers.get::<_, String>(&key).ok()?;
                    Some((key, value))
                })
                .collect()
        })
        .unwrap_or_default();

    Request {
        url: url.to_owned(),
        method: options.get("method").unwrap_or_else(|_| "GET".to_owned()),
        headers,
        body: options.get("body").ok(),
    }
}

async fn send(request: Request) -> Result<ScriptResponse, rquickjs::Error> {
    let client = reqwest::Client::new();
    let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(failed)?;
    let mut building = client.request(method, &request.url);
    for (key, value) in request.headers {
        building = building.header(key, value);
    }
    if let Some(body) = request.body {
        building = building.body(body);
    }

    let response = building.send().await.map_err(failed)?;
    let status = response.status();
    let url = response.url().to_string();
    let body = response.text().await.map_err(failed)?;

    Ok(ScriptResponse {
        status: status.as_u16(),
        ok: status.is_success(),
        url,
        body,
    })
}
