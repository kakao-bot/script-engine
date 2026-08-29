use std::error::Error;
use std::path::{Path, PathBuf};

use kakao_loco_client::api::connect;
use kakao_loco_client::core::command::NetworkType;
use script_engine::ScriptHost;

const STATE_ENV: &str = "KAKAO_LOCO_STATE_FILE";
const STATE_FILE: &str = ".kakao-loco/state.json";

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "script_engine=info,kakao_loco_client=warn".into()),
        )
        .init();

    if std::env::var_os(STATE_ENV).is_none() {
        let found = nearest_state().ok_or_else(|| format!("{STATE_FILE} 를 찾지 못했습니다."))?;
        tracing::info!(state = %found.display(), "using account state");
        unsafe { std::env::set_var(STATE_ENV, &found) };
    }

    let directory = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("scripts"), PathBuf::from);

    let runtime = tokio::runtime::Runtime::new()?;
    let local = tokio::task::LocalSet::new();
    local.block_on(&runtime, serve(&directory))
}

async fn serve(directory: &Path) -> Result<(), Box<dyn Error>> {
    let mut host = ScriptHost::load_dir(directory).await?;
    tracing::info!(count = host.len(), scripts = ?host.names(), "loaded");

    for driver in host.drivers() {
        tokio::task::spawn_local(driver);
    }

    connect::serve(NetworkType::Wifi, "", &mut host).await?;
    Ok(())
}

fn nearest_state() -> Option<PathBuf> {
    let mut directory = std::env::current_dir().ok()?;
    loop {
        let candidate = directory.join(STATE_FILE);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !directory.pop() {
            return None;
        }
    }
}
