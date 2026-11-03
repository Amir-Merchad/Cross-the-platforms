//! This device's long-lived identity (an iroh secret key) and the devices it has paired with.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Loads the 32-byte key, creating it on first run. A damaged file is an error, never a silent
/// regeneration: a new key means a new identity, and every paired device would stop trusting us.
pub fn load_or_create_key(path: &Path) -> Result<iroh::SecretKey> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let Ok(raw) = <[u8; 32]>::try_from(bytes.as_slice()) else { bail!("{} is damaged ({} bytes, expected 32)", path.display(), bytes.len()) };
            Ok(iroh::SecretKey::from_bytes(&raw))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(dir) = path.parent() { std::fs::create_dir_all(dir)?; }
            let key = iroh::SecretKey::generate();
            std::fs::write(path, key.to_bytes()).with_context(|| format!("writing {}", path.display()))?;
            Ok(key)
        }
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[derive(uniffi::Enum)]
pub enum PeerKind { Windows, Android, Ipad }

/// A paired device. `id` is its iroh endpoint id (public key) as a string.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Peer { pub id: String, pub name: String, pub kind: PeerKind }

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PairedPeers { peers: Vec<Peer> }

impl PairedPeers {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() { std::fs::create_dir_all(dir).map_err(|e| e.to_string())?; }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
    pub fn add(&mut self, p: Peer) { self.remove(&p.id); self.peers.push(p); }
    pub fn remove(&mut self, id: &str) { self.peers.retain(|p| p.id != id); }
    pub fn contains(&self, id: &str) -> bool { self.peers.iter().any(|p| p.id == id) }
    pub fn list(&self) -> &[Peer] { &self.peers }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn peer(id: &str, name: &str) -> Peer { Peer { id: id.into(), name: name.into(), kind: PeerKind::Android } }

    #[test] fn key_persists_across_loads() {
        let d = tempdir().unwrap(); let p = d.path().join("sub").join("identity.key");
        let a = load_or_create_key(&p).unwrap();
        let b = load_or_create_key(&p).unwrap();
        assert_eq!(a.public(), b.public());
    }
    #[test] fn short_key_file_is_an_error() {
        let d = tempdir().unwrap(); let p = d.path().join("identity.key");
        std::fs::write(&p, [1u8; 5]).unwrap();
        assert!(load_or_create_key(&p).is_err(), "never silently regenerate: that would unpair every device");
    }
    #[test] fn peers_round_trip() {
        let d = tempdir().unwrap(); let p = d.path().join("peers.json");
        let mut s = PairedPeers::default();
        s.add(peer("a", "S24+"));
        s.save(&p).unwrap();
        assert_eq!(PairedPeers::load(&p).list(), &[peer("a", "S24+")]);
    }
    #[test] fn add_replaces_same_id() {
        let mut s = PairedPeers::default();
        s.add(peer("a", "old")); s.add(peer("a", "new"));
        assert_eq!(s.list(), &[peer("a", "new")]);
        assert!(s.contains("a"));
        s.remove("a");
        assert!(!s.contains("a"));
    }
    #[test] fn missing_peers_file_is_empty() {
        let d = tempdir().unwrap();
        assert!(PairedPeers::load(&d.path().join("nope.json")).list().is_empty());
    }
}
