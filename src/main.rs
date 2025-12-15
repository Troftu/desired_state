mod desired_state_file;
mod error;
mod state;
mod watcher;
mod web_api;

use error::AppResult;
use state::{DesiredState, SharedState};
use std::env;
use std::io::{self, ErrorKind};
use std::path::PathBuf;

#[rocket::main]
async fn main() -> AppResult<()> {
    env_logger::init();

    let state_path = resolve_state_path()?;
    let desired_state = DesiredState::load(state_path)?;
    let shared_state: SharedState = std::sync::Arc::new(std::sync::Mutex::new(desired_state));

    watcher::spawn(shared_state.clone())?;
    web_api::launch(shared_state).await?;

    Ok(())
}

fn resolve_state_path() -> AppResult<PathBuf> {
    let mut desired_file =
        env::var("DESIRED_STATE_FILE").unwrap_or_else(|_| "desired_state.yml".to_string());

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--file" {
            let path = args
                .next()
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "--file requires a path"))?;
            desired_file = path;
        }
    }

    Ok(PathBuf::from(desired_file))
}
