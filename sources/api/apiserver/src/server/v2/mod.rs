use self::models::{ConfigurationFiles, Services};
use actix_web::{
    body::BoxBody, error::ResponseError, web, App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use datastore_ng::{MemoryDataStore, Value};
use error::Result;
use http::StatusCode;
use libservice::ServiceConfigurations;
use model::{Report, Settings};
use nix::unistd::{chown, Gid};
use snafu::{ensure, ResultExt};
use std::collections::{HashMap, HashSet};
use std::fs::{set_permissions, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, sync};
use thar_be_updates::status::UpdateStatus;

mod controller;
pub mod error;
mod legacy;
mod models;
mod ng;

pub use error::Error;

const DEFAULT_SERVICE_CONFIG_DIR: &str = "/usr/share/";

pub(crate) struct SharedData {
    // TODO switch this to a filesystem-based datastore
    pub(crate) ds: sync::RwLock<MemoryDataStore>,
    pub(crate) exec_socket_path: PathBuf,
    pub(crate) service_configuration: ServiceConfigurations,
}

pub async fn serve<P1, P2, P3>(
    socket_path: P1,
    datastore_path: P2,
    threads: usize,
    socket_gid: Option<Gid>,
    exec_socket_path: P3,
) -> Result<()>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
    P3: Into<PathBuf>,
{
    // SharedData gives us a convenient way to make data available to handler methods when it
    // doesn't come from the request itself.  It's easier than the ownership tricks required to
    // pass parameters to the handler methods.
    let shared_data = web::Data::new(SharedData {
        ds: sync::RwLock::new(MemoryDataStore::new()),
        exec_socket_path: exec_socket_path.into(),
        service_configuration: ServiceConfigurations::from_filesystem(DEFAULT_SERVICE_CONFIG_DIR)
            .await
            .context(error::ServiceConfigurationSnafu)?,
    });

    let http_server = HttpServer::new(move || {
        App::new()
            // This makes the data store available to API methods merely by having a Data
            // parameter.
            .app_data(shared_data.clone())
            .configure(legacy::register_legacy_routes)
            .configure(ng::register_ng_routes)
    })
    .workers(threads)
    .bind_uds(socket_path.as_ref())
    .context(error::BindSocketSnafu {
        path: socket_path.as_ref(),
    })?;

    // If the socket needs to be chowned to a group to grant further access, that can be passed
    // as a parameter.
    if let Some(gid) = socket_gid {
        chown(socket_path.as_ref(), None, Some(gid)).context(error::SetGroupSnafu { gid })?;
    }

    let mode = 0o0660;
    let perms = Permissions::from_mode(mode);
    set_permissions(socket_path.as_ref(), perms).context(error::SetPermissionsSnafu { mode })?;

    // Notify system manager the UNIX socket has been initialized, so other service units can proceed
    notify_unix_socket_ready()?;

    http_server.run().await.context(error::ServerStartSnafu)
}

// sd_notify helper
fn notify_unix_socket_ready() -> Result<()> {
    if env::var_os("NOTIFY_SOCKET").is_some() {
        ensure!(
            Command::new("systemd-notify")
                .arg("--ready")
                .arg("--no-block")
                .status()
                .context(error::SystemdNotifySnafu)?
                .success(),
            error::SystemdNotifyStatusSnafu
        );
        env::remove_var("NOTIFY_SOCKET");
    } else {
        info!("NOTIFY_SOCKET not set, not calling systemd-notify");
    }
    Ok(())
}

// =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=

// Can also override `render_response` if we want to change headers, content type, etc.
impl ResponseError for error::Error {
    /// Maps our error types to the HTTP error code they should return.
    fn error_response(&self) -> HttpResponse {
        use error::Error::*;
        let status_code = match self {
            // 400 Bad Request
            MissingInput { .. } => StatusCode::BAD_REQUEST,
            EmptyInput { .. } => StatusCode::BAD_REQUEST,
            NewKey { .. } => StatusCode::BAD_REQUEST,
            ReportTypeMissing { .. } => StatusCode::BAD_REQUEST,
            InvalidKey { .. } => StatusCode::BAD_REQUEST,

            // 404 Not Found
            MissingData { .. } => StatusCode::NOT_FOUND,
            ListKeys { .. } => StatusCode::NOT_FOUND,
            UpdateDoesNotExist { .. } => StatusCode::NOT_FOUND,
            NoStagedImage { .. } => StatusCode::NOT_FOUND,
            UninitializedUpdateStatus { .. } => StatusCode::NOT_FOUND,

            // 422 Unprocessable Entity
            CommitWithNoPending => StatusCode::UNPROCESSABLE_ENTITY,
            ReportNotSupported { .. } => StatusCode::UNPROCESSABLE_ENTITY,

            // 423 Locked
            UpdateShareLock { .. } => StatusCode::LOCKED,
            UpdateLockHeld { .. } => StatusCode::LOCKED,

            // 409 Conflict
            DisallowCommand { .. } => StatusCode::CONFLICT,

            // 500 Internal Server Error
            DataStoreLock => StatusCode::INTERNAL_SERVER_ERROR,
            ResponseSerialization { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            BindSocket { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ServerStart { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ListedKeyNotPresent { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            DataStore { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Deserialization { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            DataStoreSerialization { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            CommandSerialization { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            InvalidMetadata { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ConfigApplierFork { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ConfigApplierStart { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ConfigApplierStdin {} => StatusCode::INTERNAL_SERVER_ERROR,
            ConfigApplierWait { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ConfigApplierWrite { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ServiceConfiguration { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            SystemdNotify { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            SystemdNotifyStatus {} => StatusCode::INTERNAL_SERVER_ERROR,
            SetPermissions { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            SetGroup { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ReleaseData { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Shutdown { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Reboot { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            UpdateDispatcher { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            UpdateError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            UpdateStatusParse { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            UpdateInfoParse { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            UpdateLockOpen { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ReportExec { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ReportResult { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        };

        HttpResponse::build(status_code).body(self.to_string())
    }
}

/// Helper macro for implementing the actix-web Responder trait for a type.
/// $for: the type for which we implement Responder.
/// $self: just pass "self"  (macro hygiene requires this)
/// $serialize_expr: the thing to serialize for a response; this is just "self" again if $for
///    implements Serialize, or is "self.0" for a newtype over something implementing Serialize
macro_rules! impl_responder_for {
    ($for:ident, $self:ident, $serialize_expr:expr) => (
        impl Responder for $for {
            type Body = BoxBody;
            fn respond_to($self, _req: &HttpRequest) -> HttpResponse {
                let body = match serde_json::to_string(&$serialize_expr) {
                    Ok(s) => s,
                    Err(e) => return Error::ResponseSerialization { source: e }.into(),
                };
                HttpResponse::Ok()
                    .content_type("application/json")
                    .body(body)
            }
        }
    )
}

/// This lets us respond from our handler methods with a model (or Result<model>), where "model" is
/// a serde_json::Value corresponding to the Model struct.
///
/// This contains a serde_json::Value instead of a Model to support prefix queries; if the user
/// gives a prefix that doesn't match all BottlerocketRelease fields, we can't construct a
/// BottlerocketRelease since its fields aren't Option; using a Value lets us return the same
/// structure, just not including fields the user doesn't want to see.  (Trying to deserialize
/// those results into a Model/BottlerocketRelease would fail, so it's just intended for viewing.)
struct ModelResponse(serde_json::Value);
impl_responder_for!(ModelResponse, self, self.0);

/// This lets us respond from our handler methods with a Settings (or Result<Value>)
struct SettingsResponse(Value);
impl_responder_for!(SettingsResponse, self, self.0);

struct TransactionResponse(HashMap<String, HashMap<String, Value>>);
impl_responder_for!(TransactionResponse, self, self.0);

/// This lets us respond from our handler methods with a release (or Result<release>), where
/// "release" is a serde_json::Value corresponding to the BottlerocketRelease struct.
///
/// This contains a serde_json::Value instead of a BottlerocketRelease to support prefix queries;
/// if the user gives a prefix that doesn't match all BottlerocketRelease fields, we can't
/// construct a BottlerocketRelease since its fields aren't Option; using a Value lets us return
/// the same structure, just not including fields the user doesn't want to see.  (Trying to
/// deserialize those results into a BottlerocketRelease would fail, so it's just intended for
/// viewing.)
struct BottlerocketReleaseResponse(serde_json::Value);
impl_responder_for!(BottlerocketReleaseResponse, self, self.0);

/// This lets us respond from our handler methods with a HashMap (or Result<HashMap>) for metadata
struct MetadataResponse(HashMap<String, Value>);
impl_responder_for!(MetadataResponse, self, self.0);

/// This lets us respond from our handler methods with a Services (or Result<Services>)
struct ServicesResponse(Services);
impl_responder_for!(ServicesResponse, self, self.0);

/// This lets us respond from our handler methods with a UpdateStatus (or Result<UpdateStatus>)
struct UpdateStatusResponse(UpdateStatus);
impl_responder_for!(UpdateStatusResponse, self, self.0);

/// This lets us respond from our handler methods with a ConfigurationFiles (or
/// Result<ConfigurationFiles>)
struct ConfigurationFilesResponse(ConfigurationFiles);
impl_responder_for!(ConfigurationFilesResponse, self, self.0);

struct ChangedKeysResponse(HashMap<String, HashSet<String>>);
impl_responder_for!(ChangedKeysResponse, self, self.0);

struct TransactionListResponse(HashSet<String>);
impl_responder_for!(TransactionListResponse, self, self.0);

struct ReportListResponse(Vec<Report>);
impl_responder_for!(ReportListResponse, self, self.0);
