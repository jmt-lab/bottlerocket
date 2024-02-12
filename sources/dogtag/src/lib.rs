/*!
dogtag is a set of tools that detect the hostname of a bottlerocket server/instance and prints it to stdout.
if the tool is called in an environment it cannot resolve the hostname it will error out.

Currently the following hostname tools are implemented:

* 01-imds - Fetches hostname from the Instance Metadata via IMDS
* 00-reverse-dns - Uses reverse dns lookup to resolve the hostname
 */
use std::{error::Error, process::ExitCode};

use argh::FromArgs;

/// CLi defines the standard cmdline interface for all hostname handlers
#[derive(FromArgs)]
#[argh(description = "hostname resolution tool")]
pub struct Cli {
    #[argh(option)]
    #[argh(description = "ip_address of the host")]
    pub ip_address: String,
}

/// hostname_handler handles the standard execution and error logging
pub fn hostname_handler<F, E>(method: F) -> ExitCode
where
    E: Error,
    F: Fn(Cli) -> Result<String, E>,
{
    let cli: Cli = argh::from_env();
    match method(cli) {
        Ok(hostname) => {
            print!("{}", &hostname);
            ExitCode::SUCCESS
        },
        Err(e) => {
            eprintln!("{}", e);
            ExitCode::FAILURE
        }
    }
}
