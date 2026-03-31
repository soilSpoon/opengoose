//! Inode <-> host path bidirectional mapping.

use std::collections::HashMap;
use std::path::PathBuf;

pub const FUSE_ROOT_ID: u64 = 1;

#[derive(Debug, Clone)]
pub struct InodeEntry {
    pub path: PathBuf,
    pub refcount: u64,
}

pub struct InodeTable {
    next_ino: u64,
    entries: HashMap<u64, InodeEntry>,
    path_to_ino: HashMap<PathBuf, u64>,
}

impl InodeTable {
    pub fn new(root_path: PathBuf) -> Self {
        let mut entries = HashMap::new();
        let mut path_to_ino = HashMap::new();
        entries.insert(
            FUSE_ROOT_ID,
            InodeEntry {
                path: root_path.clone(),
                refcount: u64::MAX, // root never expires
            },
        );
        path_to_ino.insert(root_path, FUSE_ROOT_ID);
        InodeTable {
            next_ino: 2,
            entries,
            path_to_ino,
        }
    }

    pub fn get(&self, ino: u64) -> Option<InodeEntry> {
        self.entries.get(&ino).cloned()
    }

    /// Look up a child name under a parent inode. Returns the child's inode.
    /// Creates a new inode if not seen before.
    /// Rejects traversal names (.., absolute paths, multi-component names).
    pub fn lookup(&mut self, parent: u64, name: &str) -> Option<u64> {
        // Validate: name must be a single normal path component (no "..", "/", "a/b")
        let mut components = std::path::Path::new(name).components();
        match (components.next(), components.next()) {
            (Some(std::path::Component::Normal(_)), None) => {}
            _ => return None,
        }

        let parent_path = self.entries.get(&parent)?.path.clone();
        let child_path = parent_path.join(name);

        if let Some(&ino) = self.path_to_ino.get(&child_path) {
            if let Some(entry) = self.entries.get_mut(&ino) {
                entry.refcount = entry.refcount.saturating_add(1);
            }
            return Some(ino);
        }

        let ino = self.next_ino;
        self.next_ino += 1;
        self.entries.insert(
            ino,
            InodeEntry {
                path: child_path.clone(),
                refcount: 1,
            },
        );
        self.path_to_ino.insert(child_path, ino);
        Some(ino)
    }

    /// Create a new inode for a path (used by CREATE/MKDIR).
    pub fn insert(&mut self, parent: u64, name: &str) -> Option<u64> {
        self.lookup(parent, name)
    }

    /// Decrement refcount. Remove entry if refcount reaches 0 (not root).
    pub fn forget(&mut self, ino: u64, nlookup: u64) {
        if ino == FUSE_ROOT_ID {
            return;
        }
        if let Some(entry) = self.entries.get_mut(&ino) {
            entry.refcount = entry.refcount.saturating_sub(nlookup);
            if entry.refcount == 0 {
                let path = entry.path.clone();
                self.entries.remove(&ino);
                self.path_to_ino.remove(&path);
            }
        }
    }

    /// Get host path for an inode.
    pub fn path(&self, ino: u64) -> Option<PathBuf> {
        self.get(ino).map(|e| e.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_inode_is_one() {
        let table = InodeTable::new(PathBuf::from("/tmp/test"));
        let entry = table.get(FUSE_ROOT_ID).unwrap();
        assert_eq!(entry.path, PathBuf::from("/tmp/test"));
    }

    #[test]
    fn lookup_creates_child_inode() {
        let mut table = InodeTable::new(PathBuf::from("/tmp/test"));
        let child_ino = table.lookup(FUSE_ROOT_ID, "src");
        assert!(child_ino.is_some());
        let ino = child_ino.unwrap();
        assert_ne!(ino, FUSE_ROOT_ID);
        let entry = table.get(ino).unwrap();
        assert_eq!(entry.path, PathBuf::from("/tmp/test/src"));
    }

    #[test]
    fn lookup_same_name_returns_same_inode() {
        let mut table = InodeTable::new(PathBuf::from("/tmp/test"));
        let ino1 = table.lookup(FUSE_ROOT_ID, "file.txt").unwrap();
        let ino2 = table.lookup(FUSE_ROOT_ID, "file.txt").unwrap();
        assert_eq!(ino1, ino2);
    }

    #[test]
    fn lookup_nonexistent_parent_returns_none() {
        let mut table = InodeTable::new(PathBuf::from("/tmp/test"));
        assert!(table.lookup(999, "anything").is_none());
    }

    #[test]
    fn forget_decrements_refcount() {
        let mut table = InodeTable::new(PathBuf::from("/tmp/test"));
        let ino = table.lookup(FUSE_ROOT_ID, "file.txt").unwrap();
        assert!(table.get(ino).is_some());
        table.forget(ino, 1);
        // After forget with nlookup=1, refcount=0 -> entry removed
        assert!(table.get(ino).is_none());
        // Root survives
        assert!(table.get(FUSE_ROOT_ID).is_some());
    }

    #[test]
    fn insert_is_alias_for_lookup() {
        let mut table = InodeTable::new(PathBuf::from("/tmp/test"));
        let ino1 = table.insert(FUSE_ROOT_ID, "new.txt").unwrap();
        let ino2 = table.lookup(FUSE_ROOT_ID, "new.txt").unwrap();
        assert_eq!(ino1, ino2);
    }

    #[test]
    fn path_returns_host_path() {
        let mut table = InodeTable::new(PathBuf::from("/tmp/test"));
        let ino = table.lookup(FUSE_ROOT_ID, "deep").unwrap();
        assert_eq!(table.path(ino), Some(PathBuf::from("/tmp/test/deep")));
    }

    #[test]
    fn path_returns_none_for_unknown() {
        let table = InodeTable::new(PathBuf::from("/tmp/test"));
        assert!(table.path(999).is_none());
    }

    #[test]
    fn lookup_rejects_dotdot_traversal() {
        let mut table = InodeTable::new(PathBuf::from("/tmp/test"));
        assert!(table.lookup(FUSE_ROOT_ID, "..").is_none());
    }

    #[test]
    fn lookup_rejects_absolute_path() {
        let mut table = InodeTable::new(PathBuf::from("/tmp/test"));
        assert!(table.lookup(FUSE_ROOT_ID, "/etc/passwd").is_none());
    }

    #[test]
    fn lookup_rejects_multi_component() {
        let mut table = InodeTable::new(PathBuf::from("/tmp/test"));
        assert!(table.lookup(FUSE_ROOT_ID, "a/b").is_none());
    }

    #[test]
    fn lookup_rejects_dot() {
        let mut table = InodeTable::new(PathBuf::from("/tmp/test"));
        assert!(table.lookup(FUSE_ROOT_ID, ".").is_none());
    }
}
