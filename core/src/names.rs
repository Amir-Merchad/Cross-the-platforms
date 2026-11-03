//! Safe names for received files: a hostile or odd name never escapes the receive dir.

use std::path::{Path, PathBuf};

const RESERVED: &[&str] = &["con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9",
    "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9"];

/// `<path>.part`: where a file is written until its size and hash check out.
pub fn part_path(p: &Path) -> PathBuf { p.with_file_name(format!("{}.part", p.file_name().unwrap_or_default().to_string_lossy())) }

/// Base name only, invalid chars → `_`, reserved names prefixed, never overwrites an existing file.
pub fn safe_name(dir: &Path, wanted: &str) -> PathBuf {
    let base = wanted.rsplit(['/', '\\']).next().unwrap_or("");
    let base = base.rsplit(':').next().unwrap_or("");
    let mut name: String = base.chars().map(|c| if "<>:\"|?*".contains(c) || c.is_control() { '_' } else { c }).collect();
    name = name.trim_end_matches([' ', '.']).to_string();
    if name.is_empty() || name.chars().all(|c| c == '.') { name = "file".into(); }
    let stem_lower = name.split('.').next().unwrap_or("").to_ascii_lowercase();
    if RESERVED.contains(&stem_lower.as_str()) { name.insert(0, '_'); }

    // A name is taken while another transfer is still writing it (`<name>.part`).
    let free = |p: &PathBuf| !p.exists() && !part_path(p).exists();
    let candidate = dir.join(&name);
    if free(&candidate) { return candidate; }
    let (stem, ext) = match name.rfind('.') { Some(i) if i > 0 => (&name[..i], &name[i..]), _ => (name.as_str(), "") };
    (1..).map(|n| dir.join(format!("{stem} ({n}){ext}"))).find(free).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test] fn strips_traversal() { let d = tempdir().unwrap(); assert_eq!(safe_name(d.path(), "../../evil.txt"), d.path().join("evil.txt")); }
    #[test] fn strips_windows_path() { let d = tempdir().unwrap(); assert_eq!(safe_name(d.path(), r"C:\Windows\x.pdf"), d.path().join("x.pdf")); }
    #[test] fn reserved_name() { let d = tempdir().unwrap(); assert_eq!(safe_name(d.path(), "con.txt"), d.path().join("_con.txt")); }
    #[test] fn empty_name() { let d = tempdir().unwrap(); assert_eq!(safe_name(d.path(), ""), d.path().join("file")); }
    #[test] fn dots_only_name() { let d = tempdir().unwrap(); assert_eq!(safe_name(d.path(), ".."), d.path().join("file")); }
    #[test] fn bad_chars_replaced() { let d = tempdir().unwrap(); assert_eq!(safe_name(d.path(), "a<b>|c?.txt"), d.path().join("a_b__c_.txt")); }
    #[test] fn unicode_kept() { let d = tempdir().unwrap(); assert_eq!(safe_name(d.path(), "Übung 👋.pdf"), d.path().join("Übung 👋.pdf")); }
    #[test] fn duplicate_gets_suffix() {
        let d = tempdir().unwrap();
        std::fs::write(d.path().join("a.pdf"), b"x").unwrap();
        assert_eq!(safe_name(d.path(), "a.pdf"), d.path().join("a (1).pdf"));
        std::fs::write(d.path().join("a (1).pdf"), b"x").unwrap();
        assert_eq!(safe_name(d.path(), "a.pdf"), d.path().join("a (2).pdf"));
    }
}
