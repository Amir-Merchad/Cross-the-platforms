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

    /// Removes and returns `peer`'s batches, oldest first.
    pub fn take(&mut self, peer: &str) -> Result<Vec<Vec<PathBuf>>, String> {
        let b = self.batches.remove(peer).unwrap_or_default();
        if !b.is_empty() { self.save()?; }
        Ok(b)
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
        assert_eq!(o.take("ipad").unwrap(), vec![vec![PathBuf::from("a.pdf")], vec![PathBuf::from("b.pdf")]]);
        assert_eq!(Outbox::load(&p).pending("ipad"), 0, "taken means gone from disk too");
    }
    #[test] fn peers_are_separate() {
        let d = tempdir().unwrap(); let mut o = Outbox::load(&d.path().join("o.json"));
        o.add("a", vec!["x".into()]).unwrap();
        assert!(o.take("b").unwrap().is_empty());
        assert_eq!(o.pending("a"), 1);
    }
    #[test] fn corrupt_file_starts_empty() {
        let d = tempdir().unwrap(); let p = d.path().join("o.json");
        std::fs::write(&p, "{nope").unwrap();
        assert_eq!(Outbox::load(&p).pending("a"), 0);
    }
}
