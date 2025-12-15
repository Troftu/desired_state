use crate::{
    error::AppResult,
    state::{DesiredState, StateEvent},
};
use log::{debug, info, warn};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::env;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::time::Duration;

const EVENT_LOOP_TICK: Duration = Duration::from_secs(1);

pub fn run() -> AppResult<()> {
    let state_path = parse_args(env::args().skip(1).collect())?;
    let mut state = DesiredState::load(state_path.clone())?;
    let state_events = state.subscribe();
    let watch_target = canonicalize_for_watch(&state_path);

    let (watch_tx, watch_rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = watch_tx.send(res);
    })?;

    watcher
        .watch(&watch_target, RecursiveMode::NonRecursive)
        .map_err(|err| {
            io::Error::new(
                ErrorKind::Other,
                format!("failed to watch {}: {err}", watch_target.display()),
            )
        })?;

    info!(
        "Desired state watcher running. Monitoring '{}'",
        watch_target.display()
    );
    println!("Desired state watcher running. Press Ctrl+C to stop.");

    state.emit_current_state();
    drain_state_events(&state_events);

    loop {
        drain_state_events(&state_events);

        match watch_rx.recv_timeout(EVENT_LOOP_TICK) {
            Ok(Ok(event)) => {
                debug!(
                    "Filesystem event '{}' for '{}'",
                    format!("{:?}", event.kind),
                    format!("{:?}", event.paths)
                );
                if event_affects_target(&event, &watch_target) && is_state_change(&event.kind) {
                    if let Err(err) = state.reload_from_disk() {
                        warn!("Failed to reload desired state: '{}'", err);
                    } else {
                        info!("Reloaded desired state after file change");
                        drain_state_events(&state_events);
                    }
                }
            }
            Ok(Err(err)) => {
                warn!("File watch error: '{}'", err);
            }
            Err(RecvTimeoutError::Timeout) => {
                // no-op, loop again to keep draining events
            }
            Err(RecvTimeoutError::Disconnected) => {
                warn!("File watcher disconnected unexpectedly");
                return Err(io::Error::new(
                    ErrorKind::Other,
                    "file watcher disconnected unexpectedly",
                )
                .into());
            }
        }
    }
}

fn parse_args(args: Vec<String>) -> AppResult<PathBuf> {
    let mut desired_file =
        env::var("DESIRED_STATE_FILE").unwrap_or_else(|_| "desired_state.yml".to_string());

    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--file" => {
                let value = args
                    .get(idx + 1)
                    .ok_or_else(|| invalid_argument("--file requires a path"))?;
                desired_file = value.clone();
                idx += 2;
            }
            other => {
                return Err(invalid_argument(format!("unknown argument: {other}")).into());
            }
        }
    }

    Ok(PathBuf::from(desired_file))
}

fn canonicalize_for_watch(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn event_affects_target(event: &Event, target: &Path) -> bool {
    if event.paths.is_empty() {
        return true;
    }

    event
        .paths
        .iter()
        .any(|path| canonicalize_for_watch(path) == target)
}

fn is_state_change(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) | EventKind::Any
    )
}

fn drain_state_events(receiver: &mpsc::Receiver<StateEvent>) {
    loop {
        match receiver.try_recv() {
            Ok(event) => log_state_event(&event),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                warn!("State event channel disconnected; stopping log loop.");
                break;
            }
        }
    }
}

fn invalid_argument(msg: impl Into<String>) -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, msg.into())
}

fn log_state_event(event: &StateEvent) {
    match event {
        StateEvent::StateUpdated { version, services } => {
            println!(
                "[state-event] file version {} with {} service(s)",
                version,
                services.len()
            );
            for svc in services {
                println!("    - {} {}", svc.name, svc.version_req);
            }
        }
    }
}
