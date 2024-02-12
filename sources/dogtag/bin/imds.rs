/*!
dogtag is a set of tools that detect the hostname of a bottlerocket server/instance and prints it to stdout.
if the tool is called in an environment it cannot resolve the hostname it will error out.

Currently the following hostname tools are implemented:

* 01-imds - Fetches hostname from the Instance Metadata via IMDS
* 00-reverse-dns - Uses reverse dns lookup to resolve the hostname
 */
use dogtag::Cli;
use snafu::ResultExt;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::ExitCode;

/// Standard IPv4 IMDS Address
const IMDS_URL_V4: &str = "169.254.169.254:80";
/// Standard IPv6 IMDS Address
const IMDS_URL_V6: &str = "[fd00:ec2::254]:80";
/// Path to find the public hostname
const HOSTNAME_PATH: &str = "latest/meta-data/public-hostname";
/// Byte limit of a hostname is 253 bytes
const HOSTNAME_LIMIT: usize = 253;

/// Check if IPv6 is working, if so return the ipv6 url
/// otherwise return the IPv4
fn connect_imds(ipv6: &str, ipv4: &str) -> String {
    if TcpStream::connect(ipv6).is_ok() {
        ipv6.to_owned()
    } else {
        ipv4.to_owned()
    }
}

fn read_batch(socket: &mut TcpStream, buffer: &mut Vec<u8>) -> Result<()> {
    let mut buf = vec![0u8; HOSTNAME_LIMIT];
    let mut n: usize = HOSTNAME_LIMIT;
    while n != 0 && n == HOSTNAME_LIMIT  {
        n = socket.read(&mut buf).context(error::ReceiveSnafu)?;
        if n != 0 {
            buffer.extend_from_slice(&buf[..n]);
        }
    }
    Ok(())
}

type Result<T> = std::result::Result<T, error::Error>;

/// Simple helper type for imds communication
struct Imds(String);


impl Imds {
    /// Creates a connection to IMDS
    pub fn new() -> Self {
        Self(connect_imds(IMDS_URL_V6, IMDS_URL_V4))
    }

    #[cfg(test)]
    pub fn with_override(ipv6: &str, ipv4: &str) -> Self {
        Self(connect_imds(ipv6, ipv4))
    }
    
    /// Fetches and inserts the imdsv2 token into a request's header
    pub fn handle_token(&self, headers: &mut HashMap<String, String>) -> Result<()> {
        let (status, token_bytes) = self.send(
            "PUT",
            "latest/api/token",
            &HashMap::from([(
                "X-aws-ec2-metadata-token-ttl-seconds".to_string(),
                "1".to_string(),
            )]),
        )?;
        let token = String::from_utf8_lossy(token_bytes.as_slice()).to_string();
        snafu::ensure!(status == 200, error::FetchTokenSnafu);
    
        headers.insert("X-aws-ec2-metadata-token".to_string(), token);
        Ok(())
    }

    /// Send a request to IMDS and fetch the status code and response body
    pub fn send(&self, method: &str, path: &str, headers: &HashMap<String, String>) -> Result<(u64, Vec<u8>)> {
        // Create the tcp connection
        let mut socket = TcpStream::connect(self.0.clone()).context(error::ConnectSnafu { uri: self.0.clone()})?;

        // Format and send the headers of the request through our tcp
        // connection
        let header = format!(
            "{} /{} HTTP/1.1\r\n{}\r\n",
            method,
            path,
            headers
                .iter()
                .map(|(i, x)| format!("{}: {}\r\n", i, x))
                .collect::<Vec<_>>()
                .join("")
        );
        socket.write(header.as_bytes()).context(error::SendSnafu)?;
        socket.flush().context(error::SendSnafu)?;

        // Read the response back from tcp
        let mut buf = Vec::new();
        read_batch(&mut socket, &mut buf)?;

        // We now want to extract the headers, we get each header line by ites delim "\r\n"
        let mut header_lines: Vec<String> = Vec::new();
        let mut header_buf: Vec<u8> = Vec::new();
        let mut index = 0;
        
        while index < buf.len() {
            if index <= buf.len() - 2 && buf[index] == b'\r' && buf[index + 1] == b'\n' {
                if header_buf.is_empty() {
                    // We are at the end of our headers
                    index += 2;
                    break;
                } else {
                    let header = String::from_utf8_lossy(header_buf.as_slice()).to_string();
                    header_lines.push(header.clone());
                    header_buf = Vec::new();
                    index += 2;
                }
            } else {
                header_buf.push(buf[index]);
                index += 1;
            }
        }

        // The first line will contain the response type
        let response_status: Vec<&str> = header_lines[0].split_whitespace().collect();
        // The important part here is the part 2 status code
        let status_code = response_status[1];
        let data = buf[index..].to_vec();

        Ok((status_code.parse::<u64>().unwrap(), data))
    }

    /// Check if IMDS is v2
    pub fn is_v2(&self) -> Result<bool> {
        let (status, _) = self.send("GET", "", &HashMap::new())?;
        Ok(status == 401)
    }
}

/// Implements a hostname lookup tool by fetching the public hostname
/// from the instance metadata via IMDS. It will interface with IMDS
/// via:
///
/// * Check for IPv6, default to IPv4 if not available
/// * Check for IMDSv2, fallback to IMDSv1 if not enabled
fn run(_: Cli) -> Result<String> {
    let imds = Imds::new();
    let mut headers = HashMap::new();
    if imds.is_v2()? {
        imds.handle_token(&mut headers)?;
    }

    let (status_code, bytes) = imds.send("GET", HOSTNAME_PATH, &headers)?;
    snafu::ensure!(status_code != 404, error::UnavailableSnafu);
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

fn main() -> ExitCode {
    dogtag::hostname_handler(run)
}

mod error {
    use snafu::Snafu;

    #[derive(Debug, Snafu)]
    #[snafu(visibility(pub(super)))]
    pub(super) enum Error {
        #[snafu(display("Failed to fetch imdsv2 token"))]
        FetchToken,
        #[snafu(display(
            "IMDS unavailable, if this is unexpected please check if your instance has it enabled"
        ))]
        Unavailable,
        #[snafu(display("Error connecting to imds at {}: {}", uri, source))]
        Connect {
            uri: String,
            #[snafu(source(from(std::io::Error, Box::new)))]
            source: Box<std::io::Error>,
        },
        #[snafu(display("Error sending request to imds {}", source))]
        Send {
            #[snafu(source(from(std::io::Error, Box::new)))]
            source: Box<std::io::Error>,
        },
        #[snafu(display("Error receiving respond from imds {}", source))]
        Receive {
            #[snafu(source(from(std::io::Error, Box::new)))]
            source: Box<std::io::Error>,
        },
        #[snafu(display("Error parsing header in imds response {}", source))]
        Parse {
            #[snafu(source(from(std::io::Error, Box::new)))]
            source: Box<std::io::Error>,
        },
        #[snafu(display("Error writing hostname to console {}", source))]
        Output {
            #[snafu(source(from(std::io::Error, Box::new)))]
            source: Box<std::io::Error>,
        },
    }
}

#[cfg(test)]
mod test {
    use mockito::Server;
    use std::collections::HashMap;

    use crate::Imds;
    
    #[test]
    fn test_connect_imds_ipv6() {
        let server = Server::new();
        let url = server.host_with_port();
        let ipv6 = format!("{}", url);
        let ipv4 = "000000000"; // This should be invalid to ensure it picks ipv6 first
        let selected = super::connect_imds(&ipv6, &ipv4);
        assert_eq!(selected, url);
    }

    #[test]
    fn test_connect_imds_ipv4() {
        let server = Server::new();
        let url = server.host_with_port();
        let ipv6 = "000000000"; // This should be invalid to ensure it picks ipv4 first
        let ipv4 = format!("{}", url);
        let selected = super::connect_imds(&ipv6, &ipv4);
        assert_eq!(selected, url);
    }

    #[test]
    fn test_is_v2() {
        let mut server = Server::new();
        let mock = server.mock("GET", "/").with_status(401).create();
        let ip = server.host_with_port();
        let imds = super::Imds::with_override(&ip, &ip);
        assert!(imds.is_v2().unwrap());
        mock.assert();
    }

    #[test]
    fn test_is_not_v2() {
        let mut server = Server::new();
        let mock = server.mock("GET", "/").with_status(404).create();
        let ip = server.host_with_port();
        let imds = super::Imds::with_override(&ip, &ip);
        assert!(!imds.is_v2().unwrap());
        mock.assert();
    }

    #[test]
    fn test_send() {
        let mut server = Server::new();
        let mock = server.mock("GET", "/latest/meta-data/public-hostname")
            .with_status(200)
            .with_body("test")
            .create();
        let ip = server.host_with_port();
        let imds = super::Imds::with_override(&ip, &ip);
        let (status_code, body) = imds.send("GET", "latest/meta-data/public-hostname", &mut HashMap::new()).unwrap();
        assert_eq!(status_code, 200);
        assert_eq!(String::from_utf8_lossy(&body).to_string(), "test");
        mock.assert();
    }

    #[test]
    fn test_error_on_refusal() {
        let ip = "127.0.0.1:5547"; // TEST-NET IP
        let imds = super::Imds::with_override(ip, ip);
        let result = imds.send("GET", "latest/meta-data/public-hostname", &mut HashMap::new());
        assert!(result.is_err());
        assert!(format!("{:?}", result.unwrap_err()).starts_with("Connect"));
    }

    #[test]
    fn test_error_on_token_fail() {
        let mut server = Server::new();
        let imdsv2_token = server.mock("PUT", "/latest/api/token")
            .with_status(404)
            .create();
        let ip = server.host_with_port();
        let imds = Imds::with_override(&ip, &ip);
        let mut headers = HashMap::new();
        let result = imds.handle_token(&mut headers);
        imdsv2_token.assert();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), super::error::Error::FetchToken));
        assert!(headers.is_empty());
    }
}
