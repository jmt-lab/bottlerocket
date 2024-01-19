//! In-memory datastore for use in testing other modules.
//!
//! Mimics some of the decisions made for FilesystemDataStore, e.g. metadata being committed
//! immediately.

use std::collections::{HashMap, HashSet};

use super::{lookup_key, Committed, DataStore, Extension, Key, Result, Value};

#[derive(Debug, Default)]
pub struct MemoryDataStore {
    // Transaction name -> Extension -> Version -> Value
    pending: HashMap<String, HashMap<String, HashMap<String, Value>>>,
    // Committed (live) data.
    live: HashMap<String, HashMap<String, Value>>,
}

impl MemoryDataStore {
    pub fn new() -> Self {
        Default::default()
    }

    fn dataset(&self, committed: &Committed) -> Option<&HashMap<String, HashMap<String, Value>>> {
        match committed {
            Committed::Live => Some(&self.live),
            Committed::Pending { tx } => self.pending.get(tx),
        }
    }

    fn dataset_mut(
        &mut self,
        committed: &Committed,
    ) -> &mut HashMap<String, HashMap<String, Value>> {
        match committed {
            Committed::Live => &mut self.live,
            Committed::Pending { tx } => self.pending.entry(tx.clone()).or_default(),
        }
    }
}

impl DataStore for MemoryDataStore {
    fn list_extensions(&self, committed: &Committed) -> Result<HashMap<String, HashSet<String>>> {
        Ok(self
            .dataset(committed)
            .unwrap_or(&HashMap::new())
            .iter()
            .map(|(name, versions)| (name.clone(), versions.keys().cloned().collect()))
            .collect())
    }

    fn get_all(
        &self,
        committed: &Committed,
    ) -> Result<Option<&HashMap<String, HashMap<String, Value>>>> {
        Ok(self.dataset(committed))
    }

    fn get(&self, extension: &Extension, committed: &Committed) -> Result<Option<Value>> {
        Ok(self
            .dataset(committed)
            .unwrap_or(&HashMap::new())
            .get(&extension.name)
            .unwrap_or(&HashMap::new())
            .get(&extension.version)
            .cloned())
    }

    fn get_key(
        &self,
        extension: &Extension,
        key: &Key,
        committed: &Committed,
    ) -> Result<Option<Value>> {
        let extension_value = self.get(extension, committed)?;

        Ok(extension_value.and_then(|value| lookup_key(&value, key)))
    }

    fn set<S, Ver>(
        &mut self,
        extension_name: S,
        versioned_values: &HashMap<Ver, Value>,
        committed: &Committed,
    ) -> Result<()>
    where
        S: AsRef<str>,
        Ver: AsRef<str>,
    {
        let versioned_values = versioned_values
            .into_iter()
            .map(|(version, value)| (version.as_ref().to_owned(), value.clone()))
            .collect();

        self.dataset_mut(committed)
            .insert(extension_name.as_ref().to_owned(), versioned_values);

        Ok(())
    }

    fn commit_transaction<S>(&mut self, transaction: S) -> Result<HashMap<String, HashSet<String>>>
    where
        S: Into<String> + AsRef<str>,
    {
        // Remove anything pending for this transaction
        if let Some(pending) = self.pending.remove(transaction.as_ref()) {
            // Apply pending changes to live
            pending.iter().try_for_each(|(name, versioned_values)| {
                self.set(name.as_str(), versioned_values, &Committed::Live)?;
                Ok(())
            })?;
            // Return keys that were committed
            Ok(pending
                .into_iter()
                .map(|(name, versioned_values)| (name, versioned_values.keys().cloned().collect()))
                .collect())
        } else {
            Ok(HashMap::new())
        }
    }

    fn delete_transaction<S>(&mut self, transaction: S) -> Result<HashMap<String, HashSet<String>>>
    where
        S: Into<String> + AsRef<str>,
    {
        if let Some(pending) = self.pending.remove(transaction.as_ref()) {
            // Return the old pending keys
            Ok(pending
                .into_iter()
                .map(|(name, versioned_values)| (name, versioned_values.keys().cloned().collect()))
                .collect())
        } else {
            Ok(HashMap::new())
        }
    }

    fn list_transactions(&self) -> Result<HashSet<String>> {
        Ok(self.pending.keys().cloned().collect())
    }
}
