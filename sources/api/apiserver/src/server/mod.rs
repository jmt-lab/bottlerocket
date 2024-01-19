//! The server module owns the API surface.  It interfaces with the datastore through the
//! server::controller module.

mod exec;
pub(crate) mod v1;
pub(crate) mod v2;

const BLOODHOUND_BIN: &str = "/usr/bin/bloodhound";
const BLOODHOUND_K8S_CHECKS: &str = "/usr/libexec/cis-checks/kubernetes";

// =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=
