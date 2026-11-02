//! Native mods' world data, vanilla's `SavedData` in the overworld's `DimensionDataStorage`:
//! one compound per mod and name, kept in the world folder as `data/<mod id>/<name>.dat`
//! (`{data: {...}, DataVersion: N}`, as `data/minecraft/scoreboard.dat`). Read the first time a
//! mod asks for it, written on the world's saves only while dirty (`SavedData.setDirty`).
use super::Game;
use horizon_client::save;
use horizon_world::nbt::{Compound, Tag};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// The dimension whose storage is the world folder's `data/`.
pub(super) const OVERWORLD: &str = "minecraft:overworld";

#[derive(Default)]
pub(super) struct NativeSavedData {
    /// The world folder the entries were read from.
    root: PathBuf,
    entries: BTreeMap<(String, String), Entry>,
}

struct Entry {
    data: Compound,
    dirty: bool,
}

/// A mod id (`[a-z0-9_.-]`) and a name (the same, with `/` between non-empty parts), as
/// vanilla's `Identifier` allows them, so the file stays under `data/`.
fn valid(mod_id: &str, name: &str) -> bool {
    let part = |s: &str| {
        !s.is_empty()
            && s != "."
            && s != ".."
            && s.bytes()
                .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-'))
    };
    part(mod_id) && name.split('/').all(part)
}

fn file(root: &Path, mod_id: &str, name: &str) -> PathBuf {
    root.join("data").join(mod_id).join(format!("{name}.dat"))
}

impl NativeSavedData {
    /// A world opens at `root`: what another world's mods held is dropped.
    pub(super) fn open(&mut self, root: &Path) {
        self.root = root.to_owned();
        self.entries.clear();
    }

    /// `DimensionDataStorage.computeIfAbsent`: the compound, read from the world folder the
    /// first time (empty when there is no file). None for an invalid id or name.
    pub(super) fn get(&mut self, root: &Path, mod_id: &str, name: &str) -> Option<&mut Compound> {
        if !valid(mod_id, name) {
            return None;
        }
        if self.root != root {
            self.open(root);
        }
        let key = (mod_id.to_owned(), name.to_owned());
        let entry = self.entries.entry(key).or_insert_with(|| {
            let data = save::read_dat(&file(root, mod_id, name))
                .and_then(|t| match t.get("data") {
                    Some(Tag::Compound(c)) => Some(c.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            Entry { data, dirty: false }
        });
        Some(&mut entry.data)
    }

    /// `SavedData.setDirty`: false when the mod hasn't asked for this data in this world.
    pub(super) fn set_dirty(&mut self, mod_id: &str, name: &str) -> bool {
        let key = (mod_id.to_owned(), name.to_owned());
        self.entries.get_mut(&key).map(|e| e.dirty = true).is_some()
    }

    /// `DimensionDataStorage.saveAndJoin`: writes the dirty entries and clears their flag;
    /// an entry whose write failed stays dirty for the next save. The files written.
    pub(super) fn save(&mut self) -> Vec<PathBuf> {
        let mut written = Vec::new();
        for ((mod_id, name), entry) in self.entries.iter_mut().filter(|(_, e)| e.dirty) {
            let path = file(&self.root, mod_id, name);
            let tag = Tag::compound([
                ("data", Tag::Compound(entry.data.clone())),
                ("DataVersion", Tag::Int(horizon_world::DATA_VERSION)),
            ]);
            match save::write_dat(&path, tag) {
                Ok(()) => {
                    entry.dirty = false;
                    written.push(path);
                }
                Err(e) => log::warn!(
                    "{}: a native mod's data was not written: {e}",
                    path.display()
                ),
            }
        }
        written
    }
}

impl Game<'_> {
    /// The world's save (autosave, `/save-all`, the closing save): the mods fill their data
    /// (`native_level_save`), then the dirty files are written. Authoritative worlds only.
    pub(super) fn save_native_saved_data(&mut self) {
        if self.remote.is_some() {
            return;
        }
        self.native_level_save(OVERWORLD);
        self.native_saved.save();
    }

    /// The world opened on its server: the mods read their data (`native_level_load`).
    pub(super) fn load_native_saved_data(&mut self) {
        let root = self.world_root.clone();
        self.native_saved.open(&root);
        self.native_level_load(OVERWORLD);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mods_world_data_is_written_when_dirty_and_read_back() {
        let root = std::env::temp_dir().join(format!("horizon-saved-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let path = root.join("data/testmod/network.dat");

        // A fresh world: empty data, nothing written while it isn't dirty.
        let mut storage = NativeSavedData::default();
        storage.open(&root);
        let data = storage.get(&root, "testmod", "network").unwrap();
        assert!(data.is_empty());
        data.insert("speed".into(), Tag::Int(64));
        assert!(storage.save().is_empty());
        assert!(!path.exists());

        // Marked dirty: written once, as vanilla's `{data, DataVersion}`.
        assert!(storage.set_dirty("testmod", "network"));
        assert_eq!(storage.save(), std::slice::from_ref(&path));
        let tag = save::read_dat(&path).unwrap();
        assert_eq!(
            tag.get("DataVersion"),
            Some(&Tag::Int(horizon_world::DATA_VERSION))
        );
        assert!(storage.save().is_empty(), "clean after the write");

        // Changed but not marked: the file keeps what was saved.
        let data = storage.get(&root, "testmod", "network").unwrap();
        data.insert("speed".into(), Tag::Int(1));
        assert!(storage.save().is_empty());

        // The world opens again: the saved compound is read back.
        let mut reopened = NativeSavedData::default();
        reopened.open(&root);
        let data = reopened.get(&root, "testmod", "network").unwrap();
        assert_eq!(data.get(&"speed".into()), Some(&Tag::Int(64)));
        assert!(!reopened.set_dirty("testmod", "other"), "never asked for");

        // Ids stay under `data/`.
        assert!(reopened.get(&root, "testmod", "../level").is_none());
        assert!(reopened.get(&root, "Test", "x").is_none());
        assert!(reopened.get(&root, "testmod", "a/b").is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_world_save_and_open_reach_the_mods() {
        let game = include_str!("../game.rs");
        let meta = &game[game.find("fn save_game_metadata(&mut self)").unwrap()..];
        let meta = &meta[..meta.find("\n    fn ").unwrap()];
        assert!(meta.contains("self.save_native_saved_data();"));
        let lifecycle = include_str!("native_lifecycle.rs");
        let ready = &lifecycle[lifecycle.find("fn native_lifecycle_world_ready").unwrap()..];
        let ready = &ready[..ready.find("\n    }\n").unwrap()];
        let load = ready.find("self.load_native_saved_data();").unwrap();
        assert!(load < ready.find("WorldPhase::WorldOpenReady").unwrap());
    }
}
