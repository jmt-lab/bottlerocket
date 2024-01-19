//! Provides Bottlerocket's settings-extension-enabled API.
use super::error::{self, Result};
use super::{controller, SharedData};
use super::{
    BottlerocketReleaseResponse, ChangedKeysResponse, ConfigurationFilesResponse, MetadataResponse,
    ModelResponse, ReportListResponse, ServicesResponse, SettingsResponse, TransactionListResponse,
    TransactionResponse, UpdateStatusResponse,
};
use crate::server::{exec, BLOODHOUND_BIN, BLOODHOUND_K8S_CHECKS};
use actix_web::{web, HttpRequest, HttpResponse};
use datastore_ng::{Committed, Value};
use fs2::FileExt;
use model::{Report, Settings};
use snafu::{ensure, OptionExt, ResultExt};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::os::unix::process::ExitStatusExt;
use std::process::Command;
use thar_be_updates::status::UPDATE_LOCKFILE;
use tokio::process::Command as AsyncCommand;

pub fn register_ng_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/v1").configure(register_v1_routes));
}

pub fn register_v1_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/settings")
            .route("", web::get().to(get_settings))
            .route("", web::patch().to(patch_settings))
            .configure(|cfg| {
                // Transaction support
                cfg.service(
                    web::scope("/tx")
                        .route("/list", web::get().to(get_transaction_list))
                        .route("", web::get().to(get_transaction))
                        .route("", web::delete().to(delete_transaction))
                        .route("/commit", web::post().to(commit_transaction))
                        .route("/apply", web::post().to(apply_changes))
                        .route(
                            "/commit_and_apply",
                            web::post().to(commit_transaction_and_apply),
                        ),
                );
                // Service configuration and management
                cfg.service(
                    web::scope("/metadata")
                        .route("/affected-services", web::get().to(get_affected_services))
                        .route("/setting-generators", web::get().to(get_setting_generators))
                        .route("/templates", web::get().to(get_templates)),
                );
                cfg.service(web::scope("/services").route("", web::get().to(get_services)));
                cfg.service(
                    web::scope("/configuration-files")
                        .route("", web::get().to(get_configuration_files)),
                );
            }),
    )
    .service(web::scope("/os").route("", web::get().to(get_os_info)))
    .service(web::scope("/actions").route("/reboot", web::post().to(reboot)))
    .service(
        web::scope("/updates")
            .route("/status", web::get().to(get_update_status))
            .configure(|cfg| {
                cfg.service(
                    web::scope("/actions")
                        .route("/refresh-updates", web::post().to(refresh_updates))
                        .route("/prepare-update", web::post().to(prepare_update))
                        .route("/activate-update", web::post().to(activate_update))
                        .route("/deactivate-update", web::post().to(deactivate_update)),
                );
            }),
    )
    .service(web::resource("/exec").route(web::get().to(websocket_exec)))
    .service(
        web::scope("/report")
            .route("", web::get().to(list_reports))
            .route("/cis", web::get().to(get_cis_report)),
    );
}

// actix-web doesn't support Query for enums, so we use a HashMap and check for the expected keys
// ourselves.
/// Returns the live settings from the data store of a given set of settings extensions at specific
/// versions.
async fn get_settings(
    query: web::Query<HashMap<String, String>>,
    data: web::Data<SharedData>,
) -> Result<SettingsResponse> {
    let datastore = data.ds.read().ok().context(error::DataStoreLockSnafu)?;

    let settings = if let Some(keys_str) = query.get("extensions") {
        let keys = comma_separated("extensions", keys_str)?;
        controller::get_settings_keys(&*datastore, &keys, &Committed::Live)
    } else {
        controller::get_settings(&*datastore, &Committed::Live)
    }?;

    Ok(SettingsResponse(settings))
}

/// Apply the requested settings to the pending data store
///
/// The submitted settings JSON will look something like this:
///
/// ```json
/// {
///     "settings": {
///         "motd@v1": "hello",
///         "updates": {
///             "enabled": true
///         }
///     }
/// }
///
/// "settings" keys can be optionally versioned (e.g. "motd@v1"). If no version is
/// provided, the apiserver must inspect the settings extension's default version via
/// its configuration file and assume that the data is shaped in that version.
/// ```
async fn patch_settings(
    settings: web::Json<Value>,
    query: web::Query<HashMap<String, String>>,
    data: web::Data<SharedData>,
) -> Result<HttpResponse> {
    let transaction = transaction_name(&query);
    let mut datastore = data.ds.write().ok().context(error::DataStoreLockSnafu)?;
    controller::patch_settings(&mut *datastore, &settings, transaction)?;
    Ok(HttpResponse::NoContent().finish()) // 204
}

async fn get_transaction_list(data: web::Data<SharedData>) -> Result<TransactionListResponse> {
    let datastore = data.ds.read().ok().context(error::DataStoreLockSnafu)?;
    let data = controller::list_transactions(&*datastore)?;
    Ok(TransactionListResponse(data))
}

/// Get any pending settings in the given transaction, or the "default" transaction if unspecified.
async fn get_transaction(
    query: web::Query<HashMap<String, String>>,
    data: web::Data<SharedData>,
) -> Result<TransactionResponse> {
    let transaction = transaction_name(&query);
    let datastore = data.ds.read().ok().context(error::DataStoreLockSnafu)?;
    let data = controller::get_transaction(&*datastore, transaction)?;
    Ok(TransactionResponse(data))
}

/// Delete the given transaction, or the "default" transaction if unspecified.
async fn delete_transaction(
    query: web::Query<HashMap<String, String>>,
    data: web::Data<SharedData>,
) -> Result<ChangedKeysResponse> {
    let transaction = transaction_name(&query);
    let mut datastore = data.ds.write().ok().context(error::DataStoreLockSnafu)?;
    let deleted = controller::delete_transaction(&mut *datastore, transaction)?;
    Ok(ChangedKeysResponse(deleted))
}

/// Save settings changes from the given transaction, or the "default" transaction if unspecified,
/// to the live data store.  Returns the list of changed keys.
async fn commit_transaction(
    query: web::Query<HashMap<String, String>>,
    data: web::Data<SharedData>,
) -> Result<ChangedKeysResponse> {
    let transaction = transaction_name(&query);
    let mut datastore = data.ds.write().ok().context(error::DataStoreLockSnafu)?;

    let changes = controller::commit_transaction(&mut *datastore, transaction)?;

    if changes.is_empty() {
        return error::CommitWithNoPendingSnafu.fail();
    }

    Ok(ChangedKeysResponse(changes))
}

/// Starts settings appliers for any changes that have been committed to the data store.  This
/// updates config files, runs restart commands, etc.
async fn apply_changes(query: web::Query<HashMap<String, String>>) -> Result<HttpResponse> {
    todo!("We must implement some changes in thar-be-settings to make this work.");
    if let Some(keys_str) = query.get("keys") {
        let keys = comma_separated("keys", keys_str)?;
        controller::apply_changes(Some(&keys))?;
    } else {
        controller::apply_changes(None as Option<&HashSet<&str>>)?;
    }

    Ok(HttpResponse::NoContent().json(()))
}

/// Usually you want to apply settings changes you've committed, so this is a convenience method to
/// perform both a commit and an apply.  Commits the given transaction, or the "default"
/// transaction if unspecified.
async fn commit_transaction_and_apply(
    query: web::Query<HashMap<String, String>>,
    data: web::Data<SharedData>,
) -> Result<ChangedKeysResponse> {
    let transaction = transaction_name(&query);
    let mut datastore = data.ds.write().ok().context(error::DataStoreLockSnafu)?;

    let changes = controller::commit_transaction(&mut *datastore, transaction)?;

    if changes.is_empty() {
        return error::CommitWithNoPendingSnafu.fail();
    }

    let extension_names = changes.keys().collect();
    controller::apply_changes(Some(&extension_names))?;

    Ok(ChangedKeysResponse(changes))
}

/// Returns information about the OS image, like variant and version.  If you pass a 'prefix' query
/// string, only field names starting with that prefix will be included.  Returns a
/// BottlerocketReleaseResponse, which contains a serde_json Value instead of a BottlerocketRelease
/// so that we can include only matched fields.
async fn get_os_info(
    query: web::Query<HashMap<String, String>>,
) -> Result<BottlerocketReleaseResponse> {
    let os = if let Some(mut prefix) = query.get("prefix") {
        if prefix.is_empty() {
            return error::EmptyInputSnafu { input: "prefix" }.fail();
        }
        // When retrieving from /os, the "os" prefix is implied, so we add it if it wasn't given.
        let with_prefix = format!("os.{}", prefix);
        if !prefix.starts_with("os") {
            prefix = &with_prefix;
        }
        controller::get_os_prefix(prefix)?.unwrap_or_else(|| Value::Object(serde_json::Map::new()))
    } else {
        let os = controller::get_os_info()?;
        serde_json::to_value(os).expect("struct to value can't fail")
    };

    Ok(BottlerocketReleaseResponse(os))
}

/// Get the affected services for a list of data keys
async fn get_affected_services(
    query: web::Query<HashMap<String, String>>,
    data: web::Data<SharedData>,
) -> Result<MetadataResponse> {
    if let Some(keys_str) = query.get("keys") {
        let data_keys = comma_separated("keys", keys_str)?;

        let resp = controller::get_affected_services(
            data_keys.iter().cloned(),
            &data.service_configuration,
        )?;

        Ok(MetadataResponse(resp))
    } else {
        error::MissingInputSnafu { input: "keys" }.fail()
    }
}

/// Get all settings that have setting-generator metadata
async fn get_setting_generators(_data: web::Data<SharedData>) -> Result<MetadataResponse> {
    // Setting-generators have been offloaded to extensions
    Ok(MetadataResponse(HashMap::new()))
}

/// Get the template metadata for a list of data keys
async fn get_templates(
    _query: web::Query<HashMap<String, String>>,
    _data: web::Data<SharedData>,
) -> Result<MetadataResponse> {
    // Templates are no longer stored for a given setting
    Ok(MetadataResponse(HashMap::new()))
}

/// Get all services, or if 'names' is specified, services with those names.  If you pass a
/// 'prefix' query string, only services starting with that prefix will be included.
async fn get_services(
    query: web::Query<HashMap<String, String>>,
    data: web::Data<SharedData>,
) -> Result<ServicesResponse> {
    let resp = if let Some(names_str) = query.get("names") {
        let names = comma_separated("names", names_str)?
            .into_iter()
            .map(|name| name.trim_start_matches("services."))
            .collect();
        Ok(controller::get_services_names(
            &data.service_configuration,
            &names,
        ))
    } else if let Some(prefix) = query.get("prefix") {
        if prefix.is_empty() {
            return error::EmptyInputSnafu { input: "prefix" }.fail();
        }
        let prefix = prefix.trim_start_matches("services.");
        Ok(controller::get_services_prefix(
            &data.service_configuration,
            prefix,
        ))
    } else {
        Ok(controller::get_services(&data.service_configuration))
    }?;

    Ok(ServicesResponse(resp))
}

/// Get all configuration files, or if 'names' is specified, configuration files with those names.
/// If you pass a 'prefix' query string, only configuration files starting with that prefix will be
/// included.
async fn get_configuration_files(
    query: web::Query<HashMap<String, String>>,
    data: web::Data<SharedData>,
) -> Result<ConfigurationFilesResponse> {
    let resp = if let Some(names_str) = query.get("names") {
        let names = comma_separated("names", names_str)?
            .into_iter()
            .map(|name| name.trim_start_matches("configuration-files."))
            .collect();
        Ok(controller::get_configuration_files_names(
            &data.service_configuration,
            &names,
        ))
    } else if let Some(prefix) = query.get("prefix") {
        if prefix.is_empty() {
            return error::EmptyInputSnafu { input: "prefix" }.fail();
        }
        let prefix = prefix.trim_start_matches("configuration-files.");
        Ok(controller::get_configuration_files_prefix(
            &data.service_configuration,
            prefix,
        ))
    } else {
        Ok(controller::get_configuration_files(
            &data.service_configuration,
        ))
    }?;

    Ok(ConfigurationFilesResponse(resp))
}

/// Get the update status from 'thar-be-updates'
async fn get_update_status() -> Result<UpdateStatusResponse> {
    let lockfile = File::create(UPDATE_LOCKFILE).context(error::UpdateLockOpenSnafu)?;
    lockfile
        .try_lock_shared()
        .context(error::UpdateShareLockSnafu)?;
    let result = thar_be_updates::status::get_update_status(&lockfile);
    match result {
        Ok(update_status) => Ok(UpdateStatusResponse(update_status)),
        Err(e) => match e {
            thar_be_updates::error::Error::NoStatusFile { .. } => {
                error::UninitializedUpdateStatusSnafu.fail()
            }
            _ => error::UpdateSnafu.fail(),
        },
    }
}

/// Refreshes the list of updates and checks if an update is available matching the configured version lock
async fn refresh_updates() -> Result<HttpResponse> {
    controller::dispatch_update_command(&["refresh"])
}

/// Prepares update by downloading the images to the staging partition set
async fn prepare_update() -> Result<HttpResponse> {
    controller::dispatch_update_command(&["prepare"])
}

/// "Activates" an already staged update by bumping the priority bits on the staging partition set
async fn activate_update() -> Result<HttpResponse> {
    controller::dispatch_update_command(&["activate"])
}

/// "Deactivates" an already activated update by rolling back actions done by 'activate-update'
async fn deactivate_update() -> Result<HttpResponse> {
    controller::dispatch_update_command(&["deactivate"])
}

/// Reboots the machine
async fn reboot() -> Result<HttpResponse> {
    debug!("Rebooting now");
    let output = Command::new("/sbin/shutdown")
        .arg("-r")
        .arg("now")
        .output()
        .context(error::ShutdownSnafu)?;
    ensure!(
        output.status.success(),
        error::RebootSnafu {
            exit_code: match output.status.code() {
                Some(code) => code,
                None => output.status.signal().unwrap_or(1),
            },
            stderr: String::from_utf8_lossy(&output.stderr),
        }
    );
    Ok(HttpResponse::NoContent().finish())
}

/// Starts the WebSocket, handing control of the message stream to our WsExec actor.
pub(crate) async fn websocket_exec(
    r: HttpRequest,
    stream: web::Payload,
    data: web::Data<SharedData>,
) -> std::result::Result<HttpResponse, actix_web::Error> {
    exec::ws_exec(r, stream, &data.exec_socket_path).await
}

/// Gets the set of report types supported by this host.
async fn list_reports() -> Result<ReportListResponse> {
    // Add each report to list response when adding a new handler
    let data = vec![Report {
        name: "cis".to_string(),
        description: "CIS Bottlerocket Benchmark".to_string(),
    }];
    Ok(ReportListResponse(data))
}

/// Gets the Bottlerocket CIS benchmark report.
async fn get_cis_report(query: web::Query<HashMap<String, String>>) -> Result<HttpResponse> {
    let mut cmd = AsyncCommand::new(BLOODHOUND_BIN);

    // Check for requested level, default is 1
    if let Some(level) = query.get("level") {
        cmd.arg("-l").arg(level);
    }

    // Check for requested format, default is text
    if let Some(format) = query.get("format") {
        cmd.arg("-f").arg(format);
    }

    if let Some(report_type) = query.get("type") {
        if report_type == "kubernetes" {
            cmd.arg("-c").arg(BLOODHOUND_K8S_CHECKS);
        }
    }

    let output = cmd.output().await.context(error::ReportExecSnafu)?;
    ensure!(
        output.status.success(),
        error::ReportResultSnafu {
            exit_code: match output.status.code() {
                Some(code) => code,
                None => output.status.signal().unwrap_or(1),
            },
            stderr: String::from_utf8_lossy(&output.stderr),
        }
    );
    Ok(HttpResponse::Ok()
        .content_type("application/text")
        .body(String::from_utf8_lossy(&output.stdout).to_string()))
}

// =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=

// Helpers for handler methods called by the router

fn comma_separated<'a>(key_name: &'static str, input: &'a str) -> Result<HashSet<&'a str>> {
    if input.is_empty() {
        return error::EmptyInputSnafu { input: key_name }.fail();
    }
    Ok(input.split(',').collect())
}

fn transaction_name(query: &web::Query<HashMap<String, String>>) -> &str {
    if let Some(name_str) = query.get("tx") {
        name_str
    } else {
        "default"
    }
}
