# datastore-ng

Current version: 0.1.0

## Background

A 'data store' in Bottlerocket is used to store settings values, with the ability to commit changes in transactions.

## Library

This library provides a trait defining the exact requirements, along with basic implementations for filesystem and memory data stores.

There's also a common error type and methods that implementations of DataStore should generally share.

For each setting, for each version, we store two data parcels:
* The serialized settings object, as JSON
* A secondary object describing which of those settings are derived from system defualts

## Current Limitations
* The user (e.g. apiserver) needs to handle locking.
* There's no support for rolling back transactions.
* The `serialization` module can't handle complex types under lists; it assumes lists can be serialized as scalars.


## Colophon

This text was generated using [cargo-readme](https://crates.io/crates/cargo-readme), and includes the rustdoc from `src/lib.rs`.
