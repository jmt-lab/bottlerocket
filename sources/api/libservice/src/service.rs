//! This module contains mechanisms for loading service definitions from the filesystem.
use crate::{error, util::find_files, Result};
use futures::Stream;
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use std::path::{Path, PathBuf};
use tokio::fs;

// Services are files or symlinks (to files) ending in a common suffix
pub const SERVICE_FILE_SUFFIX: &str = ".service";

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Service {
    /// The filepath of the file or link in the services directory for this service.
    pub filepath: PathBuf,

    /// The name of the service given in the service file.
    pub name: String,

    /// The commands to issue to restart the service upon configuration change.
    pub restart_commands: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize)]
/// A target struct for deserializing service files in service of creating `Service` structs.
struct ServiceFile {
    pub name: String,
    #[serde(rename = "restart-commands")]
    pub restart_commands: Vec<String>,
}

impl Service {
    pub async fn from_file<P: AsRef<Path>>(filepath: P) -> Result<Self> {
        let filepath = filepath.as_ref().to_owned();

        let service_contents =
            fs::read_to_string(&filepath)
                .await
                .context(error::ReadFileSnafu {
                    filepath: filepath.clone(),
                })?;

        let service: ServiceFile =
            toml::de::from_str(&service_contents).context(error::ParseServiceFileSnafu {
                filepath: filepath.clone(),
            })?;

        let ServiceFile {
            name,
            restart_commands,
        } = service;

        Ok(Service {
            filepath,
            name,
            restart_commands,
        })
    }

    pub async fn find_service_files<P: AsRef<Path>>(
        services_dir: P,
    ) -> impl Stream<Item = Result<PathBuf>> {
        find_files(services_dir, |dir_entry| async move {
            let file_name = dir_entry.file_name();
            // We're only checking the suffix which is constrained to UTF-8, making it
            // acceptable to lose non-UTF-8 bytes.
            let file_name = file_name.to_string_lossy();

            // We want files or symlinks that end in our service suffix
            if file_name.ends_with(SERVICE_FILE_SUFFIX) {
                // Follow symlinks to the canonicalized file
                let canonicalized_path = fs::canonicalize(dir_entry.path()).await.context(
                    error::CanonicalizeFilepathSnafu {
                        filepath: dir_entry.path().to_owned(),
                    },
                )?;

                let file_metadata = fs::metadata(&canonicalized_path).await.context(
                    error::ReadFileMetadataSnafu {
                        filepath: dir_entry.path().to_owned(),
                    },
                )?;
                Ok(file_metadata.file_type().is_file())
            } else {
                Ok(false)
            }
        })
        .await
    }
}
