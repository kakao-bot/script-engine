use std::error::Error;
use std::path::{Path, PathBuf};

use kakao_loco_client::api::connect;
use kakao_loco_client::core::command::NetworkType;
use script_engine::ScriptHost;

const STATE_ENV: &str = "KAKAO_LOCO_STATE_FILE";
const STATE_FILE: &str = ".kakao-loco/state.json";

fn main() -> Result<(), Box<dyn Error>> {
    if std::env::var_os(STATE_ENV).is_none() {
        let found = nearest_state()
            .ok_or_else(|| format!("{STATE_FILE} 를 찾지 못했다. {STATE_ENV} 로 직접 지정해라"))?;
        println!("계정: {}", found.display());
        unsafe { std::env::set_var(STATE_ENV, &found) };
    }

    let directory = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("scripts"), PathBuf::from);

    tokio::runtime::Runtime::new()?.block_on(serve(&directory))
}

async fn serve(directory: &Path) -> Result<(), Box<dyn Error>> {
    let mut host = ScriptHost::load_dir(directory)?;
    println!("{} 개 스크립트: {:?}", host.len(), host.names());

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
