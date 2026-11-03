//! Sends to a closed iPad wait here (on disk, so they survive a PC restart) until it says `Hello`.

use std::{collections::HashMap, path::{Path, PathBuf}};

pub struct Outbox { path: PathBuf, batches: HashMap<String, Vec<Vec<PathBuf>>> }

impl Outbox {
    /// A missing or unreadable file is an empty outbox.
    pub fn load(path: &Path) -> Outbox {
        let batches = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Outbox { path: path.to_path_buf(), batches }
    }

    pub fn add(&mut self, peer: &str, paths: Vec<PathBuf>) -> Result<(), String> {
        self.batches.entry(peer.to_string()).or_default().push(paths);
        self.save()
    }

    /// `peer`'s oldest batch, still queued: `pop` it once it's delivered.
    pub fn peek(&self, peer: &str) -> Option<Vec<PathBuf>> { self.batches.get(peer).and_then(|b| b.first().cloned()) }

    /// Drops `peer`'s oldest batch.
    pub fn pop(&mut self, peer: &str) -> Result<(), String> {
        if let Some(b) = self.batches.get_mut(peer) {
            if !b.is_empty() { b.remove(0); }
            if b.is_empty() { self.batches.remove(peer); }
            self.save()?;
        }
        Ok(())
    }

    pub fn pending(&self, peer: &str) -> usize { self.batches.get(peer).map_or(0, Vec::len) }

    fn save(&self) -> Result<(), String> {
        let mut tmp = self.path.as_os_str().to_owned();
        tmp.push(".tmp");
        std::fs::write(&tmp, serde_json::to_string(&self.batches).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test] fn batches_survive_a_restart_in_order() {
        let d = tempdir().unwrap(); let p = d.path().join("outbox.json");
        let mut o = Outbox::load(&p);
        o.add("ipad", vec!["a.pdf".into()]).unwrap();
        o.add("ipad", vec!["b.pdf".into()]).unwrap();
        let mut o = Outbox::load(&p);
        assert_eq!(o.pending("ipad"), 2);
        assert_eq!(o.peek("ipad"), Some(vec![PathBuf::from("a.pdf")]));
        assert_eq!(Outbox::load(&p).pending("ipad"), 2, "peeked is still on disk");
        o.pop("ipad").unwrap();
        assert_eq!(Outbox::load(&p).peek("ipad"), Some(vec![PathBuf::from("b.pdf")]));
        o.pop("ipad").unwrap();
        assert_eq!(Outbox::load(&p).pending("ipad"), 0, "popped means gone from disk too");
    }
    #[test] fn peers_are_separate() {
        let d = tempdir().unwrap(); let mut o = Outbox::load(&d.path().join("o.json"));
        o.add("a", vec!["x".into()]).unwrap();
        assert!(o.peek("b").is_none());
        assert_eq!(o.pending("a"), 1);
    }
    #[test] fn corrupt_file_starts_empty() {
        let d = tempdir().unwrap(); let p = d.path().join("o.json");
        std::fs::write(&p, "{nope").unwrap();
        assert_eq!(Outbox::load(&p).pending("a"), 0);
    }
}
