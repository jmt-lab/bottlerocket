/*!
# Background

A 'data store' in Bottlerocket is used to store settings values, with the ability to commit changes in transactions.

# Library

This library provides a trait defining the exact requirements, along with basic implementations for filesystem and memory data stores.

There's also a common error type and methods that implementations of DataStore should generally share.

For each setting, for each version, we store two data parcels:
* The serialized settings object, as JSON
* A secondary object describing which of those settings are derived from system defualts

# Current Limitations
* The user (e.g. apiserver) needs to handle locking.
* There's no support for rolling back transactions.
* The `serialization` module can't handle complex types under lists; it assumes lists can be serialized as scalars.

*/

pub mod error;
pub mod filesystem;
pub mod key;
pub mod memory;

pub use error::{Error, Result};
// pub use filesystem::FilesystemDataStore;
pub use key::{Key, KEY_SEPARATOR, KEY_SEPARATOR_STR};
pub use memory::MemoryDataStore;

use std::collections::{HashMap, HashSet};

/// Committed represents whether we want to look at pending (uncommitted) or live (committed) data
/// in the datastore.
#[derive(Debug, Clone, PartialEq)]
pub enum Committed {
    Live,
    Pending {
        // If the change is pending, we need to know the transaction name.
        tx: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Extension {
    pub name: String,
    pub version: String,
}

pub trait DataStore {
    /// Returns a list of the stored settings extensions and each version at which they are stored.
    fn list_extensions(&self, committed: &Committed) -> Result<HashMap<String, HashSet<String>>>;

    /// Returns all stored settings at each version that they are stored
    fn get_all(
        &self,
        committed: &Committed,
    ) -> Result<Option<&HashMap<String, HashMap<String, Value>>>>;

    /// Retrieves the entire settings object for a given extension.
    fn get(&self, extension_version: &Extension, committed: &Committed) -> Result<Option<Value>>;

    /// Retrieve the value for a single data key from the datastore.
    fn get_key(
        &self,
        extension_version: &Extension,
        key: &Key,
        committed: &Committed,
    ) -> Result<Option<Value>>;

    fn set<S, Ver>(
        &mut self,
        extension: S,
        versioned_values: &HashMap<Ver, Value>,
        committed: &Committed,
    ) -> Result<()>
    where
        S: AsRef<str>,
        Ver: AsRef<str>;

    /// Applies pending changes from the given transaction to the live datastore.  Returns the
    /// list of changed keys.
    fn commit_transaction<S>(&mut self, transaction: S) -> Result<HashMap<String, HashSet<String>>>
    where
        S: Into<String> + AsRef<str>;

    /// Remove the given pending transaction from the datastore.  Returns the list of removed
    /// keys.  If the transaction doesn't exist, will return Ok with an empty list.
    fn delete_transaction<S>(&mut self, transaction: S) -> Result<HashMap<String, HashSet<String>>>
    where
        S: Into<String> + AsRef<str>;

    /// Returns a list of the names of any pending transactions in the data store.
    fn list_transactions(&self) -> Result<HashSet<String>>;
}

/// Serde generic "Value" type representing a tree of deserialized values.  Should be able to hold
/// anything returned by the deserialization bits above.
pub type Value = serde_json::Value;

// Common helper function to lookup key in a JSON object
fn lookup_key(json: &serde_json::Value, key: &Key) -> Option<serde_json::Value> {
    let mut json = json;
    for segment in key.segments() {
        json = json.get(segment)?;
    }
    Some(json.clone())
}
