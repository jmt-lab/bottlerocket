# dogtag

Current version: 0.1.0

dogtag is a set of tools that detect the hostname of a bottlerocket server/instance and prints it to stdout.
if the tool is called in an environment it cannot resolve the hostname it will error out.

Currently the following hostname tools are implemented:

* 01-imds - Fetches hostname from the Instance Metadata via IMDS
* 00-reverse-dns - Uses reverse dns lookup to resolve the hostname

## Colophon

This text was generated from `README.tpl` using [cargo-readme](https://crates.io/crates/cargo-readme), and includes the rustdoc from `src/main.rs`.
