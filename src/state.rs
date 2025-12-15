use anyhow::{Context, Result};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub name: String,
    #[serde(rename = "version")]
    pub version_req: VersionReq,
}

impl Service {
    pub fn new(name: String, version_req: VersionReq) -> Self {
        Self { name, version_req }
    }

    pub fn placeholder(name: &str) -> Self {
        Self {
            name: name.to_string(),
            version_req: VersionReq::STAR.clone(),
        }
    }
}

impl PartialEq for Service {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for Service {}

impl Hash for Service {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DesiredStateFile {
    #[serde(default = "get_current_file_version")]
    version: Version,
    #[serde(default)]
    services: Vec<Service>,
}

pub struct DesiredState {
    path: PathBuf,
    file_version: Version,
    services: HashSet<Service>,
}

impl DesiredState {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();

        let (file_version, services) = if path.exists() {
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("failed to read desired state file {}", path.display()))?;
            if raw.trim().is_empty() {
                (get_current_file_version(), HashSet::new())
            } else {
                let parsed: DesiredStateFile = serde_yaml::from_str(&raw).with_context(|| {
                    format!("failed to parse desired state file {}", path.display())
                })?;
                (parsed.version, parsed.services.into_iter().collect())
            }
        } else {
            (get_current_file_version(), HashSet::new())
        };

        let state = Self {
            path,
            file_version,
            services,
        };

        if !state.path.exists() {
            state.persist()?;
        }

        Ok(state)
    }

    pub fn list(&self) -> Vec<&Service> {
        let mut services: Vec<_> = self.services.iter().collect();
        services.sort_by(|a, b| a.name.cmp(&b.name));
        services
    }

    pub fn set_service(&mut self, name: String, version_req: VersionReq) -> Result<()> {
        let new_service = Service::new(name, version_req);
        self.services.replace(new_service);
        self.persist()
    }

    pub fn remove_service(&mut self, name: &str) -> Result<bool> {
        let placeholder = Service::placeholder(name);
        let existed = self.services.take(&placeholder).is_some();
        if existed {
            self.persist()?;
        }
        Ok(existed)
    }

    fn persist(&self) -> Result<()> {
        if let Some(parent) = self.path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }

        let mut services: Vec<_> = self.services.iter().cloned().collect();
        services.sort_by(|a, b| a.name.cmp(&b.name));

        let yaml = serde_yaml::to_string(&DesiredStateFile {
            version: self.file_version.clone(),
            services,
        })
        .with_context(|| {
            format!(
                "failed to serialize desired state to YAML for {}",
                self.path.display()
            )
        })?;

        fs::write(&self.path, yaml)
            .with_context(|| format!("failed to write desired state file {}", self.path.display()))
    }
}

fn get_current_file_version() -> Version {
    Version::new(0,1,0)
}
