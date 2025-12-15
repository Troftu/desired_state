use crate::{
    error::AppResult,
    state::{DesiredState, Service, SharedState},
};
use rocket::http::Status;
use rocket::response::status;
use rocket::serde::{Deserialize, Serialize, json::Json};
use rocket::{Build, Rocket, State, delete, get, put, routes};
use semver::VersionReq;
use std::sync::MutexGuard;

#[derive(Debug, Serialize)]
struct ServiceResponse {
    name: String,
    version: String,
}

impl From<Service> for ServiceResponse {
    fn from(service: Service) -> Self {
        Self {
            name: service.name,
            version: service.version_req.to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct SetServiceRequest {
    version: String,
}

#[get("/services")]
fn list_services(
    state: &State<SharedState>,
) -> Result<Json<Vec<ServiceResponse>>, status::Custom<String>> {
    let guard = lock_state(state)?;
    let services = guard
        .list()
        .into_iter()
        .map(ServiceResponse::from)
        .collect();
    Ok(Json(services))
}

#[put("/services/<name>", data = "<payload>")]
fn upsert_service(
    state: &State<SharedState>,
    name: String,
    payload: Json<SetServiceRequest>,
) -> Result<Json<ServiceResponse>, status::Custom<String>> {
    let version_req = VersionReq::parse(&payload.version).map_err(|err| {
        status::Custom(
            Status::BadRequest,
            format!("invalid version requirement '{}': {}", payload.version, err),
        )
    })?;

    let mut guard = lock_state(state)?;
    guard
        .set_service(name.clone(), version_req.clone())
        .map_err(internal_error)?;

    Ok(Json(ServiceResponse {
        name,
        version: version_req.to_string(),
    }))
}

#[delete("/services/<name>")]
fn delete_service(
    state: &State<SharedState>,
    name: String,
) -> Result<Status, status::Custom<String>> {
    let mut guard = lock_state(state)?;
    match guard.remove_service(&name).map_err(internal_error)? {
        true => Ok(Status::NoContent),
        false => Err(status::Custom(
            Status::NotFound,
            format!("service '{}' not found", name),
        )),
    }
}

pub async fn launch(state: SharedState) -> AppResult<()> {
    build_rocket(state)
        .launch()
        .await
        .map(|_| ())
        .map_err(|err| err.into())
}

fn build_rocket(state: SharedState) -> Rocket<Build> {
    rocket::build()
        .manage(state)
        .mount("/", routes![list_services, upsert_service, delete_service])
}

fn lock_state<'a>(
    state: &'a State<SharedState>,
) -> Result<MutexGuard<'a, DesiredState>, status::Custom<String>> {
    state
        .inner()
        .lock()
        .map_err(|_| status::Custom(Status::InternalServerError, "state lock poisoned".into()))
}

fn internal_error(err: Box<dyn std::error::Error + Send + Sync>) -> status::Custom<String> {
    status::Custom(
        Status::InternalServerError,
        format!("internal error: {}", err),
    )
}
