//! The controller module maps between the datastore and the API interface, similar to the
//! controller in the MVC model.

use bottlerocket_release::BottlerocketRelease;
use libservice::ServiceConfigurations;
use serde::de::DeserializeOwned;
use snafu::{ensure, OptionExt, ResultExt};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::process::{Command, Stdio};

use super::error::{self, Result};
use super::models::{ConfigurationFile, ConfigurationFiles, Service, Services};
use actix_web::HttpResponse;
use datastore_ng::{Committed, DataStore, Key, Value};
use model::Settings;
use num::FromPrimitive;
use std::os::unix::process::ExitStatusExt;
use thar_be_updates::error::TbuErrorStatus;

/// List the open transactions from the data store.
pub(crate) fn list_transactions<D>(datastore: &D) -> Result<HashSet<String>>
where
    D: DataStore,
{
    datastore
        .list_transactions()
        .context(error::DataStoreSnafu {
            op: "list_transactions",
        })
}

/// Build a Settings based on pending data in the datastore; the Settings will be empty if there
/// are no pending settings.
pub(crate) fn get_transaction<D, S>(
    datastore: &D,
    transaction: S,
) -> Result<HashMap<String, HashMap<String, Value>>>
where
    D: DataStore,
    S: Into<String>,
{
    let pending = Committed::Pending {
        tx: transaction.into(),
    };
    datastore
        .get_all(&pending)
        .map(|maybe_settings| maybe_settings.cloned().unwrap_or(HashMap::new()))
        .context(error::DataStoreSnafu {
            op: "get_transaction",
        })
}

/// Deletes the transaction from the data store, removing any uncommitted settings under that
/// transaction name.
pub(crate) fn delete_transaction<D: DataStore>(
    datastore: &mut D,
    transaction: &str,
) -> Result<HashMap<String, HashSet<String>>> {
    datastore
        .delete_transaction(transaction)
        .context(error::DataStoreSnafu {
            op: "delete_pending",
        })
}

/// check_prefix is a helper for get_*_prefix functions that determines what prefix to use when
/// checking whether settings match the prefix.  Pass in the prefix that was given in the API
/// request, and the expected prefix of settings in the subject area (like "settings." or
/// "services.") and it will return the prefix you should use to filter, or None if the prefix
/// can't match.
fn check_prefix<'a>(given: &'a str, expected: &'static str) -> Option<&'a str> {
    if expected.starts_with(given) {
        // Example: expected "settings." and given "se" - return "settings." since querying for
        // "se" can be ambiguous with other values ("services") that can't be deserialized into a
        // Settings.
        return Some(expected);
    }

    if given.starts_with(expected) {
        // Example: expected "settings." and given "settings.motd" - return the more specific
        // "settings.motd" so the user only gets what they clearly want to see.
        return Some(given);
    }

    // No overlap, we won't find any data and should return early.
    None
}

/// Build a Settings based on the data in the datastore.  Errors if no settings are found.
pub(crate) fn get_default_settings_view<D: DataStore>(
    _datastore: &D,
    _committed: &Committed,
) -> Result<Settings> {
    // TODO
    todo!(
        "
        * Use installed settings extensions to get the 'default' version for each
        * Fetch each extension at its default version
        * Generate an overall JSON view, e.g.
        settings
            host-containers:
                etc
            updates:
                etc
    "
    );
}

// The "os" APIs don't deal with the data store at all, they just read a release field.
/// Build a BottlerocketRelease using the bottlerocket-release library.
pub(crate) fn get_os_info() -> Result<BottlerocketRelease> {
    BottlerocketRelease::new().context(error::ReleaseDataSnafu)
}

/// Build a BottlerocketRelease using the bottlerocket-release library, returning only the fields
/// that start with the given prefix.  If the prefix was meant for another structure, we return
/// None, making it easier to decide whether to include an empty structure in API results.
pub(crate) fn get_os_prefix<S>(prefix: S) -> Result<Option<serde_json::Value>>
where
    S: AsRef<str>,
{
    let prefix = prefix.as_ref();

    // Return early if the prefix can't match os data.  (This is important because get_model checks
    // all of our model types using the same given prefix.)
    let prefix = match check_prefix(prefix, "os.") {
        Some(prefix) => prefix,
        None => return Ok(None),
    };

    // We're not using the data store here, there are no dotted keys, we're just matching against
    // field names.  Strip off the structure-level prefix.
    let field_prefix = prefix.trim_start_matches("os.");

    let os = BottlerocketRelease::new().context(error::ReleaseDataSnafu)?;

    // Turn into a serde Value we can manipulate.
    let val = serde_json::to_value(os).expect("struct to value can't fail");

    // Structs are Objects in serde_json, which have a map of field -> value inside.  We
    // destructure to get it by value, instead of as_object() which gives references.
    let map = match val {
        Value::Object(map) => map,
        _ => panic!("structs are always objects"),
    };

    // Keep the fields whose names match the requested prefix.
    let filtered = map
        .into_iter()
        .filter(|(field_name, _val)| field_name.starts_with(field_prefix))
        .collect();

    Ok(Some(filtered))
}

/// Returns all services affected by a given set of settings changes
pub(crate) fn get_affected_services<'a>(
    settings_keys: impl Iterator<Item = &'a str>,
    service_configuration: &ServiceConfigurations,
) -> Result<HashMap<String, Value>> {
    settings_keys
        .map(|settings_key| {
            let extension = requested_settings_extension(settings_key)?;

            let affected_configs =
                service_configuration.configurations_affected_by_setting(extension);

            let affected_services: HashSet<_> = affected_configs
                .flat_map(|config| {
                    service_configuration
                        .services_affected_by_config_template(config)
                        .map(|service| service.name.clone())
                })
                .collect();

            let affected_services = serde_json::to_value(affected_services)
                .context(error::ResponseSerializationSnafu)?;
            Ok((settings_key.to_owned(), affected_services))
        })
        .collect()
}

/// Determines the setting extension for each of a series of settings keys.
///
/// e.g. "settings.foo.bar" becomes "foo"
fn requested_settings_extension(settings_key: &str) -> Result<&str> {
    let mut key_parts = settings_key.split('.');
    ensure!(
        key_parts.next() == Some("settings"),
        error::InvalidKeySnafu {
            key: settings_key.to_string()
        }
    );
    key_parts.next().context(error::InvalidKeySnafu {
        key: settings_key.to_string(),
    })
}

fn serialize_service(
    service: &libservice::service::Service,
    service_configuration: &ServiceConfigurations,
) -> Service {
    Service {
        configuration_files: service_configuration
            .config_templates_for_service(service)
            .map(|config| config.name.clone())
            .collect(),
        restart_commands: service.restart_commands.clone(),
    }
}

/// Build a Services based on service files stored locally
pub(crate) fn get_services(service_configuration: &ServiceConfigurations) -> Services {
    service_configuration
        .services()
        .map(|service| {
            let name = service.name.clone();
            (name, serialize_service(service, service_configuration))
        })
        .collect()
}

/// Build a Services based on service files, returning only the fields that start
/// with the given prefix. If the prefix was meant for another structure, we return None, making it
/// easier to decide whether to include an empty structure in API results.
pub(crate) fn get_services_prefix<S: AsRef<str>>(
    service_configuration: &ServiceConfigurations,
    prefix: S,
) -> Services {
    service_configuration
        .services()
        .filter(|service| service.name.starts_with(prefix.as_ref()))
        .map(|service| {
            let name = service.name.clone();
            (name, serialize_service(service, service_configuration))
        })
        .collect()
}

/// Build a collection of Service items based on service files using only the given names
pub(crate) fn get_services_names(
    service_configuration: &ServiceConfigurations,
    names: &HashSet<&str>,
) -> Services {
    service_configuration
        .services()
        .filter(|service| names.contains(service.name.as_str()))
        .map(|service| {
            let name = service.name.clone();
            (name, serialize_service(service, service_configuration))
        })
        .collect()
}

fn serialize_configuration_files(
    config_template: &libservice::template::ConfigTemplate,
) -> Vec<ConfigurationFile> {
    config_template
        .render_destinations
        .iter()
        .map(|render_destination| ConfigurationFile {
            path: render_destination.path.to_string_lossy().to_string(),
            template_path: config_template
                .template_filepath
                .to_string_lossy()
                .to_string(),
            mode: Some(render_destination.mode.clone()),
        })
        .collect()
}

/// Build a ConfigurationFiles item based on config template files.
pub(crate) fn get_configuration_files(
    service_configuration: &ServiceConfigurations,
) -> ConfigurationFiles {
    service_configuration
        .configuration_templates()
        .flat_map(|config_template| {
            let name = config_template.name.clone();
            serialize_configuration_files(config_template)
                .into_iter()
                .map(move |config_file| (name.clone(), config_file))
        })
        .collect()
}

/// Build a ConfigurationFiles based config template files, returning only the fields that
/// start with the given prefix.  If the prefix was meant for another structure, we return None,
/// making it easier to decide whether to include an empty structure in API results.
pub(crate) fn get_configuration_files_prefix<S: AsRef<str>>(
    service_configuration: &ServiceConfigurations,
    prefix: S,
) -> ConfigurationFiles {
    service_configuration
        .configuration_templates()
        .filter(|config_template| config_template.name.starts_with(prefix.as_ref()))
        .flat_map(|config_template| {
            let name = config_template.name.clone();
            serialize_configuration_files(config_template)
                .into_iter()
                .map(move |config_file| (name.clone(), config_file))
        })
        .collect()
}

/// Build a collection of ConfigurationFile items with the given names using data from the
/// datastore.
pub(crate) fn get_configuration_files_names(
    service_configuration: &ServiceConfigurations,
    names: &HashSet<&str>,
) -> ConfigurationFiles {
    service_configuration
        .configuration_templates()
        .filter(|config_template| names.contains(config_template.name.as_str()))
        .flat_map(|config_template| {
            let name = config_template.name.clone();
            serialize_configuration_files(config_template)
                .into_iter()
                .map(move |config_file| (name.clone(), config_file))
        })
        .collect()
}

pub(crate) fn get_settings<D: DataStore>(datastore: &D, committed: &Committed) -> Result<Value> {
    todo!(
        "
        * Fetches all settings at their default value and creats a top-down view of them
        settings:
            host-containers:
                admin: etc etc
            updates:
                ignore-waves: true
                etc
    "
    )
}

pub(crate) fn get_settings_keys<D: DataStore>(
    datastore: &D,
    keys: &HashSet<&str>,
    committed: &Committed,
) -> Result<Value> {
    todo!(
        "
        * Keys are of the form settings.foo[@version].key
        * If @version is not given, we use the default version, which is specified in the
            extension config toml
    "
    )
}

/// Given a blob of settings JSON, assumes that the settings are at the "default" version and attempts
/// to apply them to the current settings..
pub(crate) fn patch_settings<D: DataStore>(
    _datastore: &mut D,
    _settings: &Value,
    _transaction: &str,
) -> Result<()> {
    // TODO
    todo!(
        "
        For all keys in the settings blob:
         * Load the current settings value at the default version
         * Patch it with the new keys given
        Then
         * Call settings extensions validators
         * Call a flood migration for each affected extension
         * Commit *all*
        "
    )
}

/// Makes live any pending settings in the datastore, returning the changed keys.
pub(crate) fn commit_transaction<D>(
    datastore: &mut D,
    transaction: &str,
) -> Result<HashMap<String, HashSet<String>>>
where
    D: DataStore,
{
    datastore
        .commit_transaction(transaction)
        .context(error::DataStoreSnafu { op: "commit" })
}

/// Launches the config applier to make appropriate changes to the system based on any settings
/// that have been committed.  Can be called after a commit, with the settings extensions that
/// changed in that commit, or called on its own to reset configuration state with all known keys.
///
/// If `settings_limit` is Some, gives those settings to the applier so only changes relevant to
/// those extensions are made.  Otherwise, tells the applier to apply changes for all known settings.
pub(crate) fn apply_changes<S>(settings_limit: Option<&HashSet<S>>) -> Result<()>
where
    S: AsRef<str>,
{
    todo!("We need to rewrite thar-be-settings to use libservice, then invoke it here.");

    if let Some(settings_limit) = settings_limit {
        let keys_limit: Vec<&str> = settings_limit.iter().map(|s| s.as_ref()).collect();
        // Prepare input to config applier; it uses the changed keys to update the right config
        trace!("Serializing the commit's changed keys: {:?}", keys_limit);
        let cmd_input =
            serde_json::to_string(&keys_limit).context(error::CommandSerializationSnafu {
                given: "commit's changed keys",
            })?;

        // Start config applier
        debug!("Launching thar-be-settings to apply changes");
        let mut cmd = Command::new("/usr/bin/thar-be-settings")
            // Ask it to fork itself so we don't block the API
            .arg("--daemon")
            .stdin(Stdio::piped())
            // FIXME where to send output?
            //.stdout()
            //.stderr()
            .spawn()
            .context(error::ConfigApplierStartSnafu)?;

        // Send changed keys to config applier
        trace!("Sending changed keys");
        cmd.stdin
            .as_mut()
            .context(error::ConfigApplierStdinSnafu)?
            .write_all(cmd_input.as_bytes())
            .context(error::ConfigApplierWriteSnafu)?;

        // The config applier forks quickly; this wait ensures we don't get a zombie from its
        // initial process.  Its child is reparented to init and init waits for that one.
        let status = cmd.wait().context(error::ConfigApplierWaitSnafu)?;
        // Similarly, this is just checking that it was able to fork, not checking its work.
        ensure!(
            status.success(),
            error::ConfigApplierForkSnafu {
                code: status
                    .code()
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
            }
        );
    } else {
        // Start config applier
        // (See comments above about daemonizing and checking the fork result; we don't need a
        // separate wait() here because we don't pass any stdin, status() does it for us.)
        debug!("Launching thar-be-settings to apply any and all changes");
        let status = Command::new("/usr/bin/thar-be-settings")
            .arg("--daemon")
            .arg("--all")
            // FIXME where to send output?
            //.stdout()
            //.stderr()
            .status()
            .context(error::ConfigApplierStartSnafu)?;
        ensure!(
            status.success(),
            error::ConfigApplierForkSnafu {
                code: status
                    .code()
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
            }
        );
    }

    Ok(())
}

/// Dispatches an update command via `thar-be-updates`
pub(crate) fn dispatch_update_command(args: &[&str]) -> Result<HttpResponse> {
    let status = Command::new("/usr/bin/thar-be-updates")
        .args(args)
        .status()
        .context(error::UpdateDispatcherSnafu)?;
    if status.success() {
        return Ok(HttpResponse::NoContent().finish());
    }
    let exit_status = match status.code() {
        Some(code) => code,
        None => status.signal().unwrap_or(1),
    };
    let error_type = FromPrimitive::from_i32(exit_status);
    let error = match error_type {
        Some(TbuErrorStatus::UpdateLockHeld) => error::Error::UpdateLockHeld,
        Some(TbuErrorStatus::DisallowCommand) => error::Error::DisallowCommand,
        Some(TbuErrorStatus::UpdateDoesNotExist) => error::Error::UpdateDoesNotExist,
        Some(TbuErrorStatus::NoStagedImage) => error::Error::NoStagedImage,
        // other errors
        _ => error::Error::UpdateError,
    };
    Err(error)
}
