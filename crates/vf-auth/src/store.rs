//! Paired browsers on disk: `pairs.json` with public keys only (not secret). Owner-only on Unix.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use vf_protocol::http::PublicJwk;

/// One paired browser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PairRecord {
    pub(crate) jwk: PublicJwk,
    pub(crate) origin: String,
    pub(crate) name: String,
    pub(crate) created: u64,
    /// Projects the browser was given access to (native dialog on first use).
    #[serde(default)]
    pub(crate) projects: Vec<String>,
}

pub(crate) struct PairStore {
    path: PathBuf,
    pairs: Mutex<BTreeMap<String, PairRecord>>,
}

impl PairStore {
    pub(crate) fn load(path: PathBuf) -> io::Result<Self> {
        let pairs = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(e),
        };
        Ok(Self {
            path,
            pairs: Mutex::new(pairs),
        })
    }

    fn pairs(&self) -> MutexGuard<'_, BTreeMap<String, PairRecord>> {
        self.pairs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn is_paired(&self, key_id: &str, origin: &str) -> bool {
        self.pairs().get(key_id).is_some_and(|r| r.origin == origin)
    }

    pub(crate) fn get(&self, key_id: &str) -> Option<PairRecord> {
        self.pairs().get(key_id).cloned()
    }

    pub(crate) fn list(&self) -> Vec<(String, PairRecord)> {
        self.pairs()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    pub(crate) fn insert(&self, key_id: String, record: PairRecord) -> io::Result<()> {
        let mut pairs = self.pairs();
        pairs.insert(key_id, record);
        self.save(&pairs)
    }

    pub(crate) fn has_project(&self, key_id: &str, project: &str) -> bool {
        self.pairs()
            .get(key_id)
            .is_some_and(|r| r.projects.iter().any(|p| p == project))
    }

    pub(crate) fn add_project(&self, key_id: &str, project: &str) -> io::Result<()> {
        let mut pairs = self.pairs();
        let Some(r) = pairs.get_mut(key_id) else {
            return Err(io::Error::other("not paired"));
        };
        if !r.projects.iter().any(|p| p == project) {
            r.projects.push(project.to_owned());
        }
        self.save(&pairs)
    }

    pub(crate) fn remove(&self, key_id: &str) -> io::Result<()> {
        let mut pairs = self.pairs();
        pairs.remove(key_id);
        self.save(&pairs)
    }

    fn save(&self, pairs: &BTreeMap<String, PairRecord>) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(pairs).map_err(io::Error::other)?,
        )?;
        restrict_to_owner(&tmp)?;
        std::fs::rename(&tmp, &self.path)
    }
}

#[cfg(unix)]
fn restrict_to_owner(path: &std::path::Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &std::path::Path) -> io::Result<()> {
    Ok(()) // the per-user data folder on Windows is already owner-only
}
