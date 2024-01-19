//! This implementation of the DataStore trait relies on the filesystem for data and metadata
//! storage.
//!
//! TODO: Currently stubbed, with some seemingly useful code from the prior implementation.

use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use snafu::OptionExt;
use std::collections::{HashMap, HashSet};
use std::path::{self, Path, PathBuf};

use super::key::Key;
use super::{error, Committed, DataStore, Result};

const METADATA_KEY_PREFIX: &str = ".";

// This describes the set of characters we encode when making the filesystem path for a given key.
// Any non-ASCII characters, plus these ones, will be encoded.
// We start off very strict (anything not alphanumeric) and remove characters we'll allow.
// To make inspecting the filesystem easier, we allow any filesystem-safe characters that are
// allowed in a Key.
const ENCODE_CHARACTERS: &AsciiSet = &NON_ALPHANUMERIC.remove(b'_').remove(b'-');

#[derive(Debug)]
pub struct FilesystemDataStore {
    live_path: PathBuf,
    pending_base_path: PathBuf,
}

impl FilesystemDataStore {
    pub fn new<P: AsRef<Path>>(base_path: P) -> FilesystemDataStore {
        FilesystemDataStore {
            live_path: base_path.as_ref().join("live"),
            pending_base_path: base_path.as_ref().join("pending"),
        }
    }

    fn base_path(&self, committed: &Committed) -> PathBuf {
        match committed {
            Committed::Pending { tx } => {
                let encoded = encode_path_component(tx);
                self.pending_base_path.join(encoded)
            }
            Committed::Live => self.live_path.clone(),
        }
    }
}

// Filesystem helpers

/// Encodes a string so that it's safe to use as a filesystem path component.
fn encode_path_component<S: AsRef<str>>(segment: S) -> String {
    let encoded = utf8_percent_encode(segment.as_ref(), ENCODE_CHARACTERS);
    encoded.to_string()
}

/// Decodes a path component, removing the encoding that's applied to make it filesystem-safe.
fn decode_path_component<S, P>(segment: S, path: P) -> Result<String>
where
    S: AsRef<str>,
    P: AsRef<Path>,
{
    let segment = segment.as_ref();

    percent_decode_str(segment)
        .decode_utf8()
        // Get back a plain String.
        .map(|cow| cow.into_owned())
        // decode_utf8 will only fail if someone messed with the filesystem contents directly
        // and created a filename that contains percent-encoded bytes that are invalid UTF-8.
        .ok()
        .context(error::CorruptionSnafu {
            path: path.as_ref(),
            msg: format!("invalid UTF-8 in encoded segment '{}'", segment),
        })
}

impl DataStore for FilesystemDataStore {
    fn list_extensions(&self, committed: &Committed) -> Result<HashMap<String, HashSet<String>>> {
        todo!()
    }

    fn get_all(
        &self,
        committed: &Committed,
    ) -> Result<Option<&HashMap<String, HashMap<String, crate::Value>>>> {
        todo!()
    }

    fn get(
        &self,
        extension_version: &crate::Extension,
        committed: &Committed,
    ) -> Result<Option<crate::Value>> {
        todo!()
    }

    fn get_key(
        &self,
        extension_version: &crate::Extension,
        key: &Key,
        committed: &Committed,
    ) -> Result<Option<crate::Value>> {
        todo!()
    }

    fn set<S, Ver>(
        &mut self,
        extension: S,
        versioned_values: &HashMap<Ver, crate::Value>,
        committed: &Committed,
    ) -> Result<()>
    where
        S: AsRef<str>,
        Ver: AsRef<str>,
    {
        todo!()
    }

    fn commit_transaction<S>(&mut self, transaction: S) -> Result<HashMap<String, HashSet<String>>>
    where
        S: Into<String> + AsRef<str>,
    {
        todo!()
    }

    fn delete_transaction<S>(&mut self, transaction: S) -> Result<HashMap<String, HashSet<String>>>
    where
        S: Into<String> + AsRef<str>,
    {
        todo!()
    }

    fn list_transactions(&self) -> Result<HashSet<String>> {
        todo!()
    }
}
