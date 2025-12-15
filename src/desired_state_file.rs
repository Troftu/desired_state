use crate::{error::AppResult, state::Service};
use log::{debug, info, warn};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DesiredStateFile {
    #[serde(default = "current_file_version")]
    version: Version,
    #[serde(default)]
    services: Vec<DesiredStateFileService>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DesiredStateFileService {
    name: String,
    version: VersionReq,
}

impl From<DesiredStateFileService> for Service {
    fn from(record: DesiredStateFileService) -> Self {
        Service {
            name: record.name,
            version_req: record.version,
        }
    }
}

impl From<&Service> for DesiredStateFileService {
    fn from(service: &Service) -> Self {
        DesiredStateFileService {
            name: service.name.clone(),
            version: service.version_req.clone(),
        }
    }
}

pub fn read(path: &Path) -> AppResult<(Version, BTreeMap<String, Service>)> {
    if !path.exists() {
        debug!(
            "Desired state file '{}' does not exist; returning empty state",
            path.display()
        );
        create_template_file(path)?;
        return Ok((current_file_version(), BTreeMap::new()));
    }

    let yaml_string = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) => {
            warn!(
                "Failed to read desired state file '{}': '{}'. Returning empty state and recreating template.",
                path.display(),
                err
            );
            create_template_file(path)?;
            return Ok((current_file_version(), BTreeMap::new()));
        }
    };

    if yaml_string.trim().is_empty() {
        debug!(
            "Desired state file '{}' is empty. Returning empty state and recreating template.",
            path.display()
        );
        create_template_file(path)?;
        return Ok((current_file_version(), BTreeMap::new()));
    }

    let parsed: DesiredStateFile = match serde_yaml::from_str(&yaml_string) {
        Ok(parsed) => parsed,
        Err(err) => {
            warn!(
                "Failed to parse desired state file '{}'. Treating as empty. Err: {}",
                path.display(),
                err
            );
            return Ok((current_file_version(), BTreeMap::new()));
        }
    };

    debug!(
        "Loaded desired state version '{}' with {} service(s) from '{}'",
        parsed.version,
        parsed.services.len(),
        path.display()
    );

    Ok((
        parsed.version,
        parsed
            .services
            .into_iter()
            .map(|record| {
                let service: Service = record.into();
                (service.name.clone(), service)
            })
            .collect(),
    ))
}

pub fn write(
    path: &Path,
    version: &Version,
    services: &BTreeMap<String, Service>,
) -> AppResult<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }

    let yaml = serde_yaml::to_string(&DesiredStateFile {
        version: version.clone(),
        services: services
            .values()
            .map(DesiredStateFileService::from)
            .collect(),
    })?;

    fs::write(path, yaml)?;

    info!(
        "Persisted desired state with {} service(s) to '{}'",
        services.len(),
        path.display()
    );
    Ok(())
}

pub fn ensure_exists(path: &Path) -> AppResult<()> {
    if path.exists() {
        return Ok(());
    }
    info!(
        "Desired state file '{}' does not exist; creating with defaults",
        path.display()
    );
    create_template_file(path)
}

fn current_file_version() -> Version {
    Version::new(0, 1, 0)
}

fn create_template_file(path: &Path) -> AppResult<()> {
    if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }

    let template_yml = DesiredStateFile {
        version: current_file_version(),
        services: vec![
            DesiredStateFileService {
                name: "example-service".to_string(),
                version: VersionReq::parse("^1.2.3")
                    .expect("static version requirement must be valid"),
            },
            DesiredStateFileService {
                name: "second-example-service".to_string(),
                version: VersionReq::parse(">0.1.0")
                    .expect("static version requirement must be valid"),
            },
        ],
    };

    let yaml = serde_yaml::to_string(&template_yml)?;
    let mut template =
        String::from("# This is an automatically generated desired state template\n");
    for line in yaml.lines() {
        template.push_str("# ");
        template.push_str(line);
        template.push('\n');
    }

    fs::write(path, template)?;
    info!("Created desired state template at '{}'", path.display());
    Ok(())
}
