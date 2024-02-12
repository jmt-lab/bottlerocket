use migration_helpers::common_migrations::AddPrefixesMigration;
use migration_helpers::{migrate, Result};
use std::process;

/// Add settings for the new certdog-toml config file
fn run() -> Result<()> {
    migrate(AddPrefixesMigration(vec![
        "services.pki",
        "configuration-files.certdog-toml",
    ]))
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", e);
        process::exit(1);
    }
}
