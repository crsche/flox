use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::ops::Deref;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use thiserror::Error;
use tracing::debug;

use crate::data::CanonicalPath;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("failed to create registry state directory: {0}")]
    CreateStateDir(#[source] std::io::Error),

    #[error("failed to create symlink: {0}")]
    CreatingLink(#[source] std::io::Error),

    #[error("failed to remove symlink: {0}")]
    RemovingLink(#[source] std::io::Error),

    #[error("failed to read registry state directory: {0}")]
    ReadingStateDir(#[source] std::io::Error),
}

/// Returns a unique identifier for the location of the project.
fn encode_path(path: &CanonicalPath) -> RegistryKey {
    RegistryKey(blake3::hash(path.as_os_str().as_bytes()).to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, derive_more::From, derive_more::AsRef)]
#[as_ref(forward)]
pub struct RegistryKey(String);

impl Display for RegistryKey {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Into<RegistryKey> for &Path {
    fn into(self) -> RegistryKey {
        encode_path(&CanonicalPath::new(self).unwrap())
    }
}

impl Into<RegistryKey> for &RegistryKey {
    fn into(self) -> RegistryKey {
        self.clone()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct RegistryEntry {
    key: RegistryKey,
    path: PathBuf,
}

impl RegistryEntry {
    pub fn key(&self) -> &RegistryKey {
        &self.key
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }
}

#[derive(Debug)]
pub struct LinkRegistry {
    /// A directory containing symlinks to registered .flox directories
    ///
    /// Symlinks may become stale if the .flox directory is moved or deleted.
    state_dir: PathBuf,
}

impl LinkRegistry {
    ///
    pub fn open(registry_state_dir: impl AsRef<Path>) -> Result<Self, RegistryError> {
        fs::create_dir_all(registry_state_dir.as_ref()).map_err(RegistryError::CreateStateDir)?;
        Ok(Self {
            state_dir: registry_state_dir.as_ref().to_path_buf(),
        })
    }

    /// Register a .flox directory
    ///
    /// Returns the id of the registered directory
    /// The ID is a semi-unique identifier for the directory.
    /// More precisely, the current implementation uses the blake3 hash of the canonicalized path.
    pub fn register(&self, path: &CanonicalPath) -> Result<RegistryKey, RegistryError> {
        let name = encode_path(path);

        let link_path = self.state_dir.join(&name);

        let Err(e) = std::os::unix::fs::symlink(path, link_path) else {
            return Ok(name);
        };

        match e.kind() {
            std::io::ErrorKind::AlreadyExists => Ok(name),
            _ => Err(RegistryError::CreatingLink(e)),
        }
    }

    /// Remove a .flox directory from the registry
    ///
    /// If the directory is not registered, this is a no-op.
    pub fn unregister(
        &self,
        key: impl Into<RegistryKey>,
    ) -> Result<Option<RegistryEntry>, RegistryError> {
        let key = key.into();
        let Some(entry) = self.get(&key) else {
            debug!(key = ?key, "entry not found, nothing to unregister");
            return Ok(None);
        };

        let link_path = self.state_dir.join(key);

        std::fs::remove_file(link_path).map_err(RegistryError::RemovingLink)?;

        Ok(Some(entry))
    }

    /// Iterate all entries in the registry
    ///
    /// Returns an iterator over all entries in the registry,
    /// that is *all symlinks in the registry directory*.
    ///
    /// The iterator yields `RegistryEntry` instances for all entries,
    /// including those that are not valid links or links to .flox directories.
    pub fn try_iter(&self) -> Result<impl Iterator<Item = RegistryEntry>, RegistryError> {
        let entries = std::fs::read_dir(&self.state_dir).map_err(RegistryError::ReadingStateDir)?;

        let iter = entries.filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path().read_link().ok()?;
            let key = path.deref().into();
            Some(RegistryEntry { key, path })
        });

        Ok(iter)
    }

    /// Get a .flox directory by its ID
    pub fn get(&self, key: &RegistryKey) -> Option<RegistryEntry> {
        let Some(target) = self.state_dir.join(key).read_link().ok() else {
            debug!(key = ?key, "link not found for requested id");
            return None;
        };

        Some(RegistryEntry {
            key: key.clone(),
            path: target,
        })
    }
}

trait Register {
    fn register(&self, registry: &LinkRegistry) -> Result<RegistryKey, RegistryError>;
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;

    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    /// Create a registry in a fresh temporary directory
    fn create_registry() -> (LinkRegistry, TempDir) {
        let tempdir = tempfile::tempdir().unwrap();
        let registry = LinkRegistry::open(tempdir.path().join("registry")).unwrap();
        (registry, tempdir)
    }

    /// Create a target directory in the temporary directory
    fn create_target_dir(tempdir: &TempDir, name: &str) -> CanonicalPath {
        let target_dir = tempdir.path().join("targets").join(name);
        fs::create_dir_all(&target_dir).unwrap();
        CanonicalPath::new(&target_dir).unwrap()
    }

    /// [encode_path] should return the same value for the same path, at different times
    #[test]
    fn test_encode_path() {
        let path = CanonicalPath::new(".").unwrap();
        let encoded = encode_path(&path);

        thread::sleep(Duration::from_secs(1));
        assert_eq!(encoded, encode_path(&path));
    }

    /// Test that paths can be registered and retrieved
    #[test]
    fn test_register_path() {
        let (registry, tempdir) = create_registry();

        let target_dir = create_target_dir(&tempdir, "test");

        let key = registry.register(&target_dir).unwrap();

        let entry = registry.get(&key).unwrap();
        assert_eq!(entry.path(), &*target_dir);
    }

    /// Test that [LinkRegistry::get] can return Entries that link to removed paths
    #[test]
    fn test_get_removed() {
        let (registry, tempdir) = create_registry();

        let target_dir = create_target_dir(&tempdir, "test");

        let key = registry.register(&target_dir).unwrap();

        let entry = registry.get(&key).unwrap();
        assert_eq!(entry.path(), &*target_dir);

        fs::remove_dir_all(&target_dir).unwrap();

        let entry = registry.get(&key).unwrap();
        assert_eq!(entry.path(), &*target_dir);
    }

    /// Test that registering the same path twice returns the same key
    #[test]
    fn test_register_path_twice_is_noop() {
        let (registry, tempdir) = create_registry();

        let target_dir = create_target_dir(&tempdir, "test");

        let key = registry.register(&target_dir).unwrap();
        let key2 = registry.register(&target_dir).unwrap();

        assert_eq!(key, key2);
    }

    /// Test that paths can be unregistered and [LinkRegistry::unregister]
    /// returns the removed entry.
    #[test]
    fn test_unregister() {
        let (registry, tempdir) = create_registry();

        let target_dir = create_target_dir(&tempdir, "test");

        let key = registry.register(&target_dir).unwrap();

        let inserted = registry.get(&key);
        assert!(inserted.is_some());

        let removed = registry.unregister(&key).unwrap();
        assert!(registry.get(&key).is_none());

        assert_eq!(inserted, removed);
    }

    /// Test that unregistering a non-existent key does not fail
    #[test]
    fn test_unregister_nonexistent() {
        let (registry, temp_dir) = create_registry();

        let removed = registry
            .unregister(temp_dir.path())
            .expect("unregistering nonexistent key is no-op");

        assert_eq!(removed, None);
    }

    /// Test that [LinkRegistry::iter] returns all registered entries
    #[test]
    fn test_iter() {
        let (registry, tempdir) = create_registry();

        let target_dir1 = create_target_dir(&tempdir, "test1");
        let target_dir2 = create_target_dir(&tempdir, "test2");

        let key1 = registry.register(&target_dir1).unwrap();
        let key2 = registry.register(&target_dir2).unwrap();

        let entries: Vec<_> = registry.try_iter().unwrap().collect();
        assert_eq!(entries.len(), 2);

        let entry1 = entries.iter().find(|e| e.key() == &key1).unwrap();
        assert_eq!(entry1.path(), &*target_dir1);

        let entry2 = entries.iter().find(|e| e.key() == &key2).unwrap();
        assert_eq!(entry2.path(), &*target_dir2);
    }
}
