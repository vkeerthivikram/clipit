use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Skip clipboard contents bigger than this (bytes).
pub const MAX_ENTRY_BYTES: usize = 100_000;
/// Skip images bigger than this (bytes).
pub const MAX_IMAGE_BYTES: usize = 4_000_000;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Text,
    Image,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    #[serde(default)]
    pub id: String,
    pub text: String,
    /// File name (inside the clipit data dir) of the image payload.
    #[serde(default)]
    pub image: Option<String>,
    pub kind: Kind,
    pub pinned: bool,
    #[serde(default)]
    pub ts: u64,
}

/// Clipboard history, stored newest-first. Pinned entries are shown first.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct History {
    entries: Vec<Entry>,
}

impl History {
    pub fn load() -> Self {
        let Ok(mut file) = std::fs::File::open(path()) else {
            return Self::default();
        };
        let mut buf = String::new();
        if file.read_to_string(&mut buf).is_err() {
            return Self::default();
        }
        let mut history: History = serde_json::from_str(&buf).unwrap_or_default();
        let dir = data_dir();
        // Backfill ids/timestamps from older versions and drop image entries
        // whose payload file vanished.
        let now = now_secs();
        history.entries.retain_mut(|e| {
            if e.id.is_empty() {
                e.id = hash_hex(e.text.as_bytes());
            }
            if e.ts == 0 {
                e.ts = now;
            }
            if e.kind == Kind::Image {
                return image_path(&dir, e.image.as_deref())
                    .map(|p| p.is_file())
                    .unwrap_or(false);
            }
            true
        });
        history
    }

    pub fn save(&self) {
        let path = path();
        if let Some(dir) = path.parent()
            && let Err(why) = std::fs::create_dir_all(dir)
        {
            eprintln!("clipit: cannot create data dir: {why}");
            return;
        }
        match serde_json::to_string(&self.entries) {
            Ok(json) => {
                if let Err(why) = std::fs::write(path, json) {
                    eprintln!("clipit: cannot save history: {why}");
                }
            }
            Err(why) => eprintln!("clipit: cannot serialize history: {why}"),
        }
    }

    /// Add text as the newest entry, unless it matches an ignore pattern.
    /// Returns whether the entry was recorded.
    pub fn add_text(&mut self, text: String, ignore: &[String]) -> bool {
        let lower = text.to_lowercase();
        if ignore
            .iter()
            .filter(|p| !p.trim().is_empty())
            .any(|p| lower.contains(p.trim().to_lowercase().as_str()))
        {
            return false;
        }
        let id = hash_hex(text.as_bytes());
        self.insert(Entry {
            id,
            text,
            image: None,
            kind: Kind::Text,
            pinned: false,
            ts: now_secs(),
        });
        true
    }

    /// Add image bytes as the newest entry. Returns whether it was recorded.
    pub fn add_image(&mut self, bytes: &[u8]) -> bool {
        let id = hash_hex(bytes);
        let dir = data_dir();
        if let Err(why) = std::fs::create_dir_all(dir.join("images")) {
            eprintln!("clipit: cannot create image dir: {why}");
            return false;
        }
        let file = format!("{id}.png");
        let target = dir.join("images").join(&file);
        if !target.is_file()
            && let Err(why) = std::fs::write(&target, bytes)
        {
            eprintln!("clipit: cannot write image: {why}");
            return false;
        }
        self.insert(Entry {
            id,
            text: String::new(),
            image: Some(file),
            kind: Kind::Image,
            pinned: false,
            ts: now_secs(),
        });
        true
    }

    fn insert(&mut self, mut entry: Entry) {
        if let Some(existing) = self.entries.iter_mut().find(|e| e.id == entry.id) {
            entry.pinned = existing.pinned;
        }
        self.entries.retain(|e| e.id != entry.id);
        self.entries.insert(0, entry);
    }

    pub fn delete(&mut self, id: &str) -> Option<Entry> {
        let pos = self.entries.iter().position(|e| e.id == id)?;
        Some(self.entries.remove(pos))
    }

    /// Re-add a previously deleted entry at the front (undo).
    pub fn restore(&mut self, entry: Entry) {
        self.insert(entry);
    }

    pub fn toggle_pin(&mut self, id: &str) {
        for e in &mut self.entries {
            if e.id == id {
                e.pinned = !e.pinned;
            }
        }
    }

    /// Remove unpinned entries; pinned entries survive.
    pub fn clear(&mut self) {
        self.entries.retain(|e| e.pinned);
    }

    /// Keep at most `max` unpinned entries; pinned entries never trimmed.
    pub fn trim(&mut self, max: usize) {
        let mut unpinned = 0;
        self.entries.retain(|e| {
            if e.pinned {
                true
            } else {
                unpinned += 1;
                unpinned <= max
            }
        });
    }

    /// Drop unpinned entries older than `expire_days` (0 disables).
    pub fn prune(&mut self, expire_days: u64) {
        if expire_days == 0 {
            return;
        }
        let cutoff = now_secs().saturating_sub(expire_days * 86_400);
        self.entries
            .retain(|e| e.pinned || e.ts >= cutoff);
    }

    /// Display order: pinned first (most recent first inside each group).
    pub fn display(&self) -> Vec<Entry> {
        let mut entries = self.entries.clone();
        entries.sort_by_key(|e| !e.pinned);
        entries
    }

    pub fn total(&self) -> usize {
        self.entries.len()
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn pinned_count(&self) -> usize {
        self.entries.iter().filter(|e| e.pinned).count()
    }
}

pub fn data_dir() -> PathBuf {
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(value) => PathBuf::from(value),
        None => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share"),
    };
    base.join("clipit")
}

fn path() -> PathBuf {
    data_dir().join("history.json")
}

pub fn image_path(dir: &std::path::Path, file: Option<&str>) -> Option<PathBuf> {
    file.map(|f| dir.join("images").join(f))
}

pub fn hash_hex(bytes: &[u8]) -> String {
    format!("{:016x}", crc64(bytes))
}

/// FNV-1a 64-bit: dependency-free, stable across runs.
fn crc64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Relative age label; empty for entries without a timestamp.
pub fn rel_time(ts: u64) -> String {
    if ts == 0 {
        return String::new();
    }
    let secs = now_secs().saturating_sub(ts);
    if secs < 60 {
        "now".into()
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86_400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupe_moves_to_front_without_duplicates() {
        let mut h = History::default();
        assert!(h.add_text("a".into(), &[]));
        assert!(h.add_text("b".into(), &[]));
        assert!(h.add_text("a".into(), &[]));
        let items = h.display();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text, "a");
    }

    #[test]
    fn re_copied_entry_keeps_pin_state() {
        let mut h = History::default();
        h.add_text("a".into(), &[]);
        h.toggle_pin(&hash_hex(b"a"));
        h.add_text("b".into(), &[]);
        h.add_text("a".into(), &[]);
        let items = h.display();
        assert_eq!(items[0].text, "a");
        assert!(items[0].pinned);
    }

    #[test]
    fn clear_keeps_pinned_only() {
        let mut h = History::default();
        h.add_text("a".into(), &[]);
        h.add_text("b".into(), &[]);
        h.toggle_pin(&hash_hex(b"b"));
        h.clear();
        assert_eq!(h.total(), 1);
        assert_eq!(h.display()[0].text, "b");
    }

    #[test]
    fn trim_respects_pins_and_max() {
        let mut h = History::default();
        h.add_text("a".into(), &[]);
        h.add_text("b".into(), &[]);
        h.add_text("c".into(), &[]);
        h.add_text("p".into(), &[]);
        h.toggle_pin(&hash_hex(b"p"));
        h.trim(2);
        assert_eq!(h.total(), 3);
        let texts: Vec<String> = h.display().into_iter().map(|e| e.text).collect();
        assert!(texts.contains(&"p".to_string()));
        assert!(!texts.contains(&"a".to_string()));
    }

    #[test]
    fn ignore_patterns_block_capture() {
        let mut h = History::default();
        let patterns = vec!["secret".to_string(), "Token:".to_string()];
        assert!(!h.add_text("my secret data".into(), &patterns));
        assert!(!h.add_text("Token: abc123".into(), &patterns));
        assert!(h.add_text("harmless".into(), &patterns));
        assert_eq!(h.total(), 1);
    }

    #[test]
    fn delete_returns_entry_and_undo_restores() {
        let mut h = History::default();
        h.add_text("a".into(), &[]);
        h.add_text("b".into(), &[]);
        let removed = h.delete(&hash_hex(b"b")).unwrap();
        assert_eq!(h.total(), 1);
        h.restore(removed);
        assert_eq!(h.display()[0].text, "b");
    }

    #[test]
    fn prune_drops_old_unpinned_keeps_pinned() {
        let mut h = History::default();
        h.add_text("old".into(), &[]);
        h.entries[0].ts = 1;
        h.add_text("pinned-old".into(), &[]);
        h.entries[0].ts = 1;
        h.toggle_pin(&hash_hex(b"pinned-old"));
        h.add_text("fresh".into(), &[]);
        h.prune(1);
        let texts: Vec<String> = h.display().into_iter().map(|e| e.text).collect();
        assert!(texts.contains(&"fresh".to_string()));
        assert!(texts.contains(&"pinned-old".to_string()));
        assert!(!texts.contains(&"old".to_string()));
    }
}
