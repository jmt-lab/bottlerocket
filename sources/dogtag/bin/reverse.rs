/*!
dogtag is a set of tools that detect the hostname of a bottlerocket server/instance and prints it to stdout.
if the tool is called in an environment it cannot resolve the hostname it will error out.

Currently the following hostname tools are implemented:

* 01-imds - Fetches hostname from the Instance Metadata via IMDS
* 00-reverse-dns - Uses reverse dns lookup to resolve the hostname
 */
use std::process::ExitCode;

use dns_lookup::lookup_addr;
use dogtag::Cli;
use snafu::ResultExt;

type Result<T> = std::result::Result<T, error::Error>;

/// Looks up the public hostname by using dns-lookup to
/// resolve it from the ip address provided
fn run(cli: Cli) -> Result<String> {
    let ip: std::net::IpAddr = cli.ip_address.parse().context(error::InvalidIpSnafu)?;
    lookup_addr(&ip).context(error::LookupSnafu)
}

fn main() -> ExitCode {
    dogtag::hostname_handler(run)
}

mod error {
    use snafu::Snafu;

    #[derive(Debug, Snafu)]
    #[snafu(visibility(pub(super)))]
    pub(super) enum Error {
        #[snafu(display("Invalid ip address passed to tool {}", source))]
        InvalidIp {
            #[snafu(source(from(std::net::AddrParseError, Box::new)))]
            source: Box<std::net::AddrParseError>,
        },
        #[snafu(display("Failed to lookup hostname via dns {}", source))]
        Lookup {
            #[snafu(source(from(std::io::Error, Box::new)))]
            source: Box<std::io::Error>,
        },
    }
}
