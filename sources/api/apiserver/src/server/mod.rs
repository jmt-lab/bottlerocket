//! The server module owns the API surface.  It interfaces with the datastore through the
//! server::controller module.

mod controller;
mod error;
mod exec;
mod v1;
mod v2;

pub use error::Error;

use error::Result;
use log::info;
use snafu::{ensure, ResultExt};
use std::env;
use std::process::Command;

const BLOODHOUND_BIN: &str = "/usr/bin/bloodhound";
const BLOODHOUND_K8S_CHECKS: &str = "/usr/libexec/cis-checks/kubernetes";

// =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=

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

// Router
/// This is the primary interface of the module.  It defines the server and application that actix
/// spawns for requests.  It creates a shared datastore handle that can be used by handler methods
/// to interface with the controller.
#[cfg(not(feature = "settings-extensions"))]
pub use v1::serve;
#[cfg(feature = "settings-extensions")]
pub use v2::serve;
