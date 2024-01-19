//! libservice is a Rust library designed to load service definitions and their configurations
//! managed by Bottlerocket's settings sytem.
use crate::service::Service;
use crate::template::ConfigTemplate;
use futures::StreamExt;
use snafu::ResultExt;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

pub mod service;
pub mod template;
mod util;

pub use error::Error;

// Services and configuration templates are stored under subdirectories of a single root.
const TEMPLATES_ROOT_PATH: &str = "templates";
const SERVICES_ROOT_PATH: &str = "services";

/// Provides an interface for querying the services and configurations installed on the system.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ServiceConfigurations {
    /// The set of services installed in the system services root.
    /// In Bottlerocket, this is typically `/sys-root/usr/share/services/`
    services: HashMap<PathBuf, Service>,
    /// The set of configuration templates installed in the system templates root.
    /// In Bottlerocket, this is typically `/sys-root/usr/share/templates/`
    config_templates: Vec<ConfigTemplate>,
}

impl ServiceConfigurations {
    pub fn services(&self) -> impl Iterator<Item = &Service> {
        self.services.values()
    }

    pub fn configuration_templates(&self) -> impl Iterator<Item = &ConfigTemplate> {
        self.config_templates.iter()
    }

    /// Returns the set of congirution templates which must be re-rendered upon changes to a given
    /// setting.
    pub fn configurations_affected_by_setting<'a>(
        &'a self,
        settings_extension_name: &str,
    ) -> impl Iterator<Item = &'a ConfigTemplate> + '_ {
        let settings_extension_name = settings_extension_name.to_owned();

        self.config_templates.iter().filter(move |config_template| {
            config_template
                .template
                .frontmatter
                .extension_requirements()
                .any(|extension_requirement| extension_requirement.name == settings_extension_name)
        })
    }

    /// Returns the set of services which must be restarted when a given configuration template has
    /// changed.
    pub fn services_affected_by_config_template<'a>(
        &'a self,
        config_template: &'a ConfigTemplate,
    ) -> impl Iterator<Item = &Service> + '_ {
        config_template
            .affected_services
            .iter()
            .filter_map(|service_path| self.services.get(service_path))
    }

    /// Returns the set of configuration templates associated with a given service.
    pub fn config_templates_for_service<'a>(
        &'a self,
        service: &'a Service,
    ) -> impl Iterator<Item = &ConfigTemplate> + '_ {
        self.config_templates.iter().filter(move |config_template| {
            config_template
                .affected_services
                .contains(&service.filepath)
        })
    }
}

impl ServiceConfigurations {
    /// Loads service and configuration definitions from a given share directory.
    /// On Bottlerocket, this directory is typically `/sys-root/usr/share/`.
    pub async fn from_filesystem<P: AsRef<Path>>(share_dir: P) -> Result<Self> {
        let services: HashMap<PathBuf, Service> = Self::load_services(&share_dir)
            .await?
            .into_iter()
            .map(|service| (service.filepath.clone(), service))
            .collect();

        let config_templates = Self::load_config_templates(&share_dir, &services).await?;

        Ok(ServiceConfigurations {
            services,
            config_templates,
        })
    }

    /// Loads service definitions from a given share directory.
    async fn load_services<P: AsRef<Path>>(root_dir: P) -> Result<Vec<Service>> {
        let mut services = Vec::new();
        let services_dir = root_dir.as_ref().join(SERVICES_ROOT_PATH);

        let mut services_file_paths = Box::pin(Service::find_service_files(services_dir).await);

        while let Some(service_file_path) = services_file_paths.next().await {
            let service_file_path = service_file_path?;
            let service_file_path = fs::canonicalize(&service_file_path).await.context(
                error::CanonicalizeFilepathSnafu {
                    filepath: service_file_path.clone(),
                },
            )?;
            let service = Service::from_file(&service_file_path).await?;
            services.push(service);
        }

        Ok(services)
    }

    /// Loads configuration templates from a given share directory.
    async fn load_config_templates<P: AsRef<Path>>(
        root_dir: P,
        services: &HashMap<PathBuf, Service>,
    ) -> Result<Vec<ConfigTemplate>> {
        let mut config_templates = Vec::new();

        let templates_dir = root_dir.as_ref().join(TEMPLATES_ROOT_PATH);
        let mut template_file_paths =
            Box::pin(ConfigTemplate::find_template_files(&templates_dir).await);

        while let Some(template_file_path) = template_file_paths.next().await {
            let template_file_path = template_file_path?;
            config_templates.push(
                ConfigTemplate::from_file(&template_file_path, &templates_dir, services).await?,
            );
        }

        Ok(config_templates)
    }
}

mod error {
    use crate::template::ParseRenderDestinationError;
    use snafu::Snafu;
    use std::path::PathBuf;

    #[derive(Debug, Snafu)]
    #[snafu(visibility(pub))]
    pub enum Error {
        #[snafu(display(
            "Failed to canonicalize file path '{}': {}",
            filepath.to_string_lossy(), source
        ))]
        CanonicalizeFilepath {
            source: std::io::Error,
            filepath: PathBuf,
        },

        #[snafu(display(
            "Failed to parse render destination '{}': {}",
            filepath.to_string_lossy(), source
        ))]
        ParseRenderDestination {
            source: ParseRenderDestinationError,
            filepath: PathBuf,
        },

        #[snafu(display(
            "Failed to parse service file '{}': {}",
            filepath.to_string_lossy(), source
        ))]
        ParseServiceFile {
            source: toml::de::Error,
            filepath: PathBuf,
        },

        #[snafu(display(
            "Failed to parse config template '{}': {}",
            filepath.to_string_lossy(), source
        ))]
        ParseTemplate {
            source: schnauzer::template::Error,
            filepath: PathBuf,
        },

        #[snafu(display(
            "Failed to read file metadata '{}': {}",
            filepath.to_string_lossy(), source
        ))]
        ReadFileMetadata {
            source: std::io::Error,
            filepath: PathBuf,
        },

        #[snafu(display(
            "Failed to read from file '{}': {}",
            filepath.to_string_lossy(), source
        ))]
        ReadFile {
            source: std::io::Error,
            filepath: PathBuf,
        },

        #[snafu(display(
            "Failed to read from directory '{}': {}",
            directory.to_string_lossy(), source
        ))]
        TraverseDirectory {
            source: std::io::Error,
            directory: PathBuf,
        },
    }
}

pub(crate) type Result<T> = std::result::Result<T, error::Error>;
