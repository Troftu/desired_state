use crate::{desired_state_file, error::AppResult};
use semver::{Version, VersionReq};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    pub name: String,
    pub version_req: VersionReq,
}

impl Service {
    pub fn new(name: String, version_req: VersionReq) -> Self {
        Self { name, version_req }
    }
}

#[derive(Debug, Clone)]
pub enum StateEvent {
    StateUpdated {
        version: Version,
        services: Vec<Service>,
    },
}

pub type StateEventReceiver = Receiver<StateEvent>;

#[derive(Default)]
struct EventHub {
    listeners: Vec<Sender<StateEvent>>,
}

impl EventHub {
    fn subscribe(&mut self) -> StateEventReceiver {
        let (tx, rx) = mpsc::channel();
        self.listeners.push(tx);
        rx
    }

    fn emit(&mut self, event: StateEvent) {
        self.listeners
            .retain(|sender| sender.send(event.clone()).is_ok());
    }
}

pub struct DesiredState {
    path: PathBuf,
    file_version: Version,
    services: BTreeMap<String, Service>,
    events: EventHub,
}

impl DesiredState {
    pub fn load(path: impl Into<PathBuf>) -> AppResult<Self> {
        let path = path.into();

        let (file_version, services) = desired_state_file::read(&path)?;

        let state = Self {
            path,
            file_version,
            services,
            events: EventHub::default(),
        };

        desired_state_file::ensure_exists(&state.path)?;

        Ok(state)
    }

    pub fn subscribe(&mut self) -> StateEventReceiver {
        self.events.subscribe()
    }

    pub fn reload_from_disk(&mut self) -> AppResult<()> {
        let (file_version, services) = desired_state_file::read(&self.path)?;
        let changed = file_version != self.file_version || services != self.services;
        if changed {
            self.file_version = file_version;
            self.services = services;
            self.notify_listeners();
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn list(&self) -> Vec<&Service> {
        self.services.values().collect()
    }

    #[allow(dead_code)]
    pub fn set_service(&mut self, name: String, version_req: VersionReq) -> AppResult<()> {
        let service = Service::new(name, version_req);
        self.services.insert(service.name.clone(), service);
        desired_state_file::write(&self.path, &self.file_version, &self.services)?;
        self.notify_listeners();
        Ok(())
    }

    #[allow(dead_code)]
    pub fn remove_service(&mut self, name: &str) -> AppResult<bool> {
        let existed = self.services.remove(name).is_some();
        if existed {
            desired_state_file::write(&self.path, &self.file_version, &self.services)?;
            self.notify_listeners();
        }
        Ok(existed)
    }

    pub fn emit_current_state(&mut self) {
        self.notify_listeners();
    }

    fn notify_listeners(&mut self) {
        let event = StateEvent::StateUpdated {
            version: self.file_version.clone(),
            services: self.snapshot_services(),
        };
        self.events.emit(event);
    }

    fn snapshot_services(&self) -> Vec<Service> {
        self.services.values().cloned().collect()
    }
}
