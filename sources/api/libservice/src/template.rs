//! This module contains mechanisms for loading configuration templates from the filesystem.
use crate::service::{Service, SERVICE_FILE_SUFFIX};
use crate::{error, util::find_files, Result};
use futures::{Stream, StreamExt};
use schnauzer::template::Template;
use snafu::{OptionExt, ResultExt};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tokio::fs;

// Templates are files or symlinks (to files) ending in a common suffix
const TEMPLATE_FILE_SUFFIX: &str = ".template";
const TEMPLATE_AFFECTED_SERVICES_SUFFIX: &str = "template.affected-services";
const TEMPLATE_RENDER_DESTINATION_SUFFIX: &str = "template.rendered-to";

// The default filemode for rendered templates if none is given.
const DEFAULT_RENDER_DESTINATION_MODE: &str = "0644";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConfigTemplate {
    /// The path to the template file.
    pub template_filepath: PathBuf,
    pub template: Template,
    pub affected_services: Vec<PathBuf>,
    pub render_destinations: Vec<RenderDestination>,
}

impl ConfigTemplate {
    pub async fn from_file<P1, P2>(
        filepath: P1,
        templates_dir: P2,
        services: &HashMap<PathBuf, Service>,
    ) -> Result<Self>
    where
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let template_filepath = filepath.as_ref().to_owned();
        let template_str =
            fs::read_to_string(&template_filepath)
                .await
                .context(error::ReadFileSnafu {
                    filepath: template_filepath.clone(),
                })?;
        let template: Template = template_str.parse().context(error::ParseTemplateSnafu {
            filepath: template_filepath.clone(),
        })?;

        let affected_services_dir = templates_dir
            .as_ref()
            .join(&template_filepath)
            .with_extension(TEMPLATE_AFFECTED_SERVICES_SUFFIX);

        let affected_services =
            Self::load_affected_services(&affected_services_dir, services).await?;

        let render_configs_dir = templates_dir
            .as_ref()
            .join(&template_filepath)
            .with_extension(TEMPLATE_RENDER_DESTINATION_SUFFIX);
        let render_destinations = Self::load_render_destinations(&render_configs_dir).await?;

        Ok(ConfigTemplate {
            template_filepath,
            template,
            affected_services,
            render_destinations,
        })
    }

    async fn load_affected_services<P: AsRef<Path>>(
        affected_services_dir: P,
        service_lookup: &HashMap<PathBuf, Service>,
    ) -> Result<Vec<PathBuf>> {
        let mut affected_services_file_paths = Box::pin(
            find_files(&affected_services_dir, |dir_entry| async move {
                // Find symlinks that end in `.service` and reside in the service directory.
                let file_name = dir_entry.file_name();
                // We're only checking the suffix which is constrained to UTF-8, making it
                // acceptable to lose non-UTF-8 bytes.
                let file_name = file_name.to_string_lossy();

                let file_type =
                    dir_entry
                        .file_type()
                        .await
                        .context(error::ReadFileMetadataSnafu {
                            filepath: dir_entry.path().to_owned(),
                        })?;

                if file_name.ends_with(SERVICE_FILE_SUFFIX) && file_type.is_symlink() {
                    let linked_path = fs::canonicalize(dir_entry.path()).await.context(
                        error::CanonicalizeFilepathSnafu {
                            filepath: dir_entry.path().to_owned(),
                        },
                    )?;
                    Ok(service_lookup.contains_key(linked_path.as_path()))
                } else {
                    Ok(false)
                }
            })
            .await,
        );

        let mut affected_services = Vec::new();
        while let Some(affected_service_file_path) = affected_services_file_paths.next().await {
            let affected_service_file_path = affected_service_file_path?;
            // These are guaranteed to be symlinks pointing to service files that we know about.
            let affected_service_path = fs::canonicalize(&affected_service_file_path)
                .await
                .context(error::CanonicalizeFilepathSnafu {
                    filepath: affected_service_file_path.clone(),
                })?;

            affected_services.push(affected_service_path);
        }

        Ok(affected_services)
    }

    async fn load_render_destinations<P: AsRef<Path>>(
        render_destinations_dir: P,
    ) -> Result<Vec<RenderDestination>> {
        let mut render_destination_files = Box::pin(
            find_files(render_destinations_dir, |dir_entry| async move {
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
            })
            .await,
        );

        let mut render_destinations = Vec::new();
        while let Some(render_destination_file_path) = render_destination_files.next().await {
            let render_destination_file_path = render_destination_file_path?;
            render_destinations
                .append(&mut RenderDestination::from_file(&render_destination_file_path).await?);
        }

        Ok(render_destinations)
    }

    pub async fn find_template_files<P: AsRef<Path>>(
        templates_dir: P,
    ) -> impl Stream<Item = Result<PathBuf>> {
        find_files(templates_dir, |dir_entry| async move {
            let file_name = dir_entry.file_name();
            // We're only checking the suffix which is constrained to UTF-8, making it
            // acceptable to lose non-UTF-8 bytes.
            let file_name = file_name.to_string_lossy();

            // We want files or symlinks that end in our template suffix
            if file_name.ends_with(TEMPLATE_FILE_SUFFIX) {
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

/// Defines a location to which a config template should be rendered.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct RenderDestination {
    /// The path to which the template should be rendered.
    pub path: PathBuf,
    /// The mode that the config file should be created with.
    pub mode: String,
    /// The owning user that the config file should be created with. Can be a user ID or a username.
    pub user: Option<String>,
    /// The owning group that the config file should be created with. Can be a group ID or a group
    /// name.
    pub group: Option<String>,
}

impl RenderDestination {
    pub async fn from_file<P: AsRef<Path>>(filepath: P) -> Result<Vec<Self>> {
        let render_destination_str =
            fs::read_to_string(&filepath.as_ref())
                .await
                .context(error::ReadFileSnafu {
                    filepath: filepath.as_ref().to_owned(),
                })?;

        render_destination_str
            .trim()
            .lines()
            .filter(|line| !line.starts_with('#'))
            .map(|line| {
                line.parse().context(error::ParseRenderDestinationSnafu {
                    filepath: filepath.as_ref().to_owned(),
                })
            })
            .collect()
    }
}

impl FromStr for RenderDestination {
    type Err = ParseRenderDestinationError;

    /// RenderDestinations are given in the form path[ mode[ user[ group]]]
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let render_destination_parts: Vec<_> = s.split_ascii_whitespace().collect();
        snafu::ensure!(
            !render_destination_parts.is_empty() && render_destination_parts.len() <= 4,
            parse_render_dest_error::IncorrectAritySnafu {
                arity: render_destination_parts.len()
            }
        );

        let mut render_destination_parts = render_destination_parts.into_iter();
        let path = PathBuf::from(render_destination_parts.next().context(
            parse_render_dest_error::IncorrectAritySnafu {
                arity: render_destination_parts.len(),
            },
        )?);

        // Allows the '-' character to be provided to indicate to use the default value.
        let map_default = |s: String| -> Option<String> {
            if s == "-" {
                None
            } else {
                Some(s)
            }
        };

        let mode = render_destination_parts
            .next()
            .map(str::to_string)
            .and_then(map_default)
            .unwrap_or(DEFAULT_RENDER_DESTINATION_MODE.to_string());

        // Ensure that the given mode is valid
        let is_octal = |c: char| c.is_ascii_digit() && c != '8' && c != '9';
        snafu::ensure!(
            mode.len() == 4 && mode.chars().all(is_octal),
            parse_render_dest_error::InvalidModeSnafu { mode: mode.clone() }
        );

        let user = render_destination_parts
            .next()
            .map(str::to_string)
            .and_then(map_default);
        let group = render_destination_parts
            .next()
            .map(str::to_string)
            .and_then(map_default);

        Ok(RenderDestination {
            path,
            mode,
            user,
            group,
        })
    }
}

mod parse_render_dest_error {
    use snafu::Snafu;

    #[derive(Debug, Snafu)]
    #[snafu(visibility(pub))]
    pub enum ParseRenderDestinationError {
        #[snafu(display(
            "Incorrect arity for render destination. [1-4] elements required, {} given.",
            arity
        ))]
        IncorrectArity { arity: usize },

        #[snafu(display("Given mode '{}' is invalid: Must be a 4 digit octal number.", mode))]
        InvalidMode { mode: String },
    }
}

pub use parse_render_dest_error::ParseRenderDestinationError;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_render_destination_only_path() {
        let input = " path";
        let expected = RenderDestination {
            path: "path".into(),
            mode: DEFAULT_RENDER_DESTINATION_MODE.to_string(),
            user: None,
            group: None,
        };

        let parsed: RenderDestination = input.parse().unwrap();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_render_destination_path_and_mode() {
        let input = "path 0755";
        let expected = RenderDestination {
            path: "path".into(),
            mode: "0755".to_string(),
            user: None,
            group: None,
        };

        let parsed: RenderDestination = input.parse().unwrap();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_render_destination_path_mode_and_user() {
        let input = "path 0644     user  ";
        let expected = RenderDestination {
            path: "path".into(),
            mode: "0644".to_string(),
            user: Some("user".to_string()),
            group: None,
        };

        let parsed: RenderDestination = input.parse().unwrap();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_render_destination_path_mode_user_and_group() {
        let input = "path 0700  user group";
        let expected = RenderDestination {
            path: "path".into(),
            mode: "0700".to_string(),
            user: Some("user".to_string()),
            group: Some("group".to_string()),
        };

        let parsed: RenderDestination = input.parse().unwrap();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_render_destination_explicit_default_mode_and_user() {
        let input = "path - -";
        let expected = RenderDestination {
            path: "path".into(),
            mode: DEFAULT_RENDER_DESTINATION_MODE.to_string(),
            user: None,
            group: None,
        };

        let parsed: RenderDestination = input.parse().unwrap();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_render_destination_explicit_defaults_all() {
        let input = "path - - -";
        let expected = RenderDestination {
            path: "path".into(),
            mode: DEFAULT_RENDER_DESTINATION_MODE.to_string(),
            user: None,
            group: None,
        };

        let parsed: RenderDestination = input.parse().unwrap();
        assert_eq!(parsed, expected);
    }
}
