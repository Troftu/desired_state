use crate::{
    error::AppResult,
    state::{DesiredState, SharedState, StateEvent},
};
use log::{debug, info, warn};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::thread;
use std::time::Duration;

const EVENT_LOOP_TICK: Duration = Duration::from_secs(1);

pub fn spawn(state: SharedState) -> AppResult<()> {
    let watcher_state = state.clone();
    thread::Builder::new()
        .name("desired-state-watcher".into())
        .spawn(move || {
            if let Err(err) = watch_loop(watcher_state) {
                warn!("File watcher stopped: '{}'", err);
            }
        })?;
    Ok(())
}

fn watch_loop(state: SharedState) -> AppResult<()> {
    let events = {
        let guard = lock_state(&state)?;
        guard.subscribe()
    };

    {
        let mut guard = lock_state(&state)?;
        guard.emit_current_state();
    }

    let file_path = {
        let guard = lock_state(&state)?;
        guard.path()
    };

    let watch_target = canonicalize_for_watch(&file_path);

    let (watch_tx, watch_rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = watch_tx.send(res);
    })?;

    watcher.watch(&watch_target, RecursiveMode::NonRecursive)?;

    info!("Watching desired state file '{}'", watch_target.display());

    loop {
        drain_state_events(&events);
        match watch_rx.recv_timeout(EVENT_LOOP_TICK) {
            Ok(Ok(event)) => {
                debug!(
                    "Filesystem event '{}' for '{}'",
                    format!("{:?}", event.kind),
                    format!("{:?}", event.paths)
                );
                if event_affects_target(&event, &watch_target) && is_state_change(&event.kind) {
                    let result = {
                        let mut guard = lock_state(&state)?;
                        guard.reload_from_disk()
                    };

                    match result {
                        Ok(()) => {
                            info!("Reloaded desired state after file change");
                            drain_state_events(&events);
                        }
                        Err(err) => {
                            warn!("Failed to reload desired state: '{}'", err);
                        }
                    }
                }
            }
            Ok(Err(err)) => {
                warn!("File watch error: '{}'", err);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err(io::Error::new(
                    ErrorKind::Other,
                    "file watcher disconnected unexpectedly",
                )
                .into());
            }
        }
    }
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

fn log_state_event(event: &StateEvent) {
    match event {
        StateEvent::StateUpdated { version, services } => {
            info!(
                "State updated to version '{}' with {} service(s)",
                version,
                services.len()
            );
            for svc in services {
                info!("    - {} {}", svc.name, svc.version_req);
            }
        }
    }
}

fn lock_state<'a>(state: &'a SharedState) -> AppResult<std::sync::MutexGuard<'a, DesiredState>> {
    Ok(state
        .lock()
        .map_err(|_| io::Error::new(ErrorKind::Other, "state lock poisoned"))?)
}
