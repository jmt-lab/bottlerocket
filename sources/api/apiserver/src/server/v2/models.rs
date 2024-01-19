//! Models served by the apiserver.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Note: Top-level objects that get returned from the API should have a "rename" attribute
// matching the struct name, but in kebab-case, e.g. ConfigurationFiles -> "configuration-files".
// This lets it match the datastore name.
// Objects that live inside those top-level objects, e.g. Service lives in Services, should have
// rename="" so they don't add an extra prefix to the datastore path that doesn't actually exist.
// This is important because we have APIs that can return those sub-structures directly.

/// Internal services
pub type Services = HashMap<String, Service>;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", rename = "")]
pub struct Service {
    pub configuration_files: Vec<String>,
    pub restart_commands: Vec<String>,
}

pub type ConfigurationFiles = HashMap<String, ConfigurationFile>;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", rename = "")]
pub struct ConfigurationFile {
    pub path: String,
    pub template_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}
