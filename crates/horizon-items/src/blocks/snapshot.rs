//! A disposable snapshot of the decoded block facts (`gameplay-facts.json` is
//! about 50 MB, twice that with content mods, and decoding it is the longest
//! step of the core tables). The source files stay authoritative: the snapshot
//! is keyed by their length and modification time and by the running
//! executable's, and any mismatch, read error or corrupt file falls back to the
//! normal decode. It lives in the game's cache folder, never beside the shared
//! resources.
use super::{BlockFacts, Fluid, GameplayProjection, RenderShape, Sounds, StateFacts};
use horizon_world::registry::BlockStateId;
use std::{
    collections::HashMap,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    time::UNIX_EPOCH,
};

const MAGIC: &[u8; 8] = b"HZFACTS\0";
const VERSION: u32 = 1;
const FILE_NAME: &str = "block-facts-v1.bin";
static DIR: OnceLock<PathBuf> = OnceLock::new();

/// Keep snapshots in `dir` (the client passes `<game folder>/.horizon-cache`).
/// Without it (tests, tools, servers), facts are always decoded from the JSON.
pub fn set_dir(dir: PathBuf) {
    let _ = DIR.set(dir);
}

/// The sources' identity: the tables the facts are read from, their length
/// and modification time, the registry's size, and the executable's.
fn key(root: &Path, states: usize) -> Option<u64> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    VERSION.hash(&mut h);
    states.hash(&mut h);
    let exe = std::env::current_exe().ok();
    let files = ["gameplay-facts.json", "block-behaviours.json", "facts.json"];
    for path in files.iter().map(|f| root.join(f)).chain(exe) {
        let m = fs::metadata(&path).ok()?;
        path.hash(&mut h);
        m.len().hash(&mut h);
        m.modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_nanos()
            .hash(&mut h);
    }
    Some(h.finish())
}

/// The snapshot for `root`, if one matches it.
pub(super) fn load(root: &Path, states: usize) -> Option<(BlockFacts, Vec<GameplayProjection>)> {
    let dir = DIR.get()?;
    let key = key(root, states)?;
    let bytes = fs::read(dir.join(FILE_NAME)).ok()?;
    decode(&bytes, key, states)
}

/// Writes the snapshot for `root` (errors are ignored: it is only a cache).
pub(super) fn store(root: &Path, facts: &BlockFacts, projection: &[GameplayProjection]) {
    let (Some(dir), Some(key)) = (DIR.get(), key(root, facts.states.len())) else {
        return;
    };
    let bytes = encode(facts, projection, key);
    let _ = fs::create_dir_all(dir);
    let tmp = dir.join(format!("{FILE_NAME}.{}.tmp", std::process::id()));
    if fs::write(&tmp, &bytes).is_ok() {
        let _ = fs::rename(&tmp, dir.join(FILE_NAME));
    }
}

#[derive(Default)]
struct Writer {
    out: Vec<u8>,
    strings: Vec<Arc<str>>,
    string_ids: HashMap<Arc<str>, u32>,
    classes: Vec<Arc<[Arc<str>]>>,
    class_ids: HashMap<Vec<Arc<str>>, u32>,
    boxes: Vec<Arc<[[f64; 6]]>>,
    box_ids: HashMap<Vec<[u64; 6]>, u32>,
}

impl Writer {
    fn u8(&mut self, v: u8) {
        self.out.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn string(&mut self, s: &Arc<str>) -> u32 {
        if let Some(&i) = self.string_ids.get(s) {
            return i;
        }
        let i = self.strings.len() as u32;
        self.strings.push(s.clone());
        self.string_ids.insert(s.clone(), i);
        i
    }
    fn class(&mut self, c: &Arc<[Arc<str>]>) -> u32 {
        let k: Vec<Arc<str>> = c.to_vec();
        if let Some(&i) = self.class_ids.get(&k) {
            return i;
        }
        for s in c.iter() {
            self.string(s);
        }
        let i = self.classes.len() as u32;
        self.classes.push(c.clone());
        self.class_ids.insert(k, i);
        i
    }
    fn boxes(&mut self, b: &Arc<[[f64; 6]]>) -> u32 {
        let k: Vec<[u64; 6]> = b.iter().map(|r| r.map(f64::to_bits)).collect();
        if let Some(&i) = self.box_ids.get(&k) {
            return i;
        }
        let i = self.boxes.len() as u32;
        self.boxes.push(b.clone());
        self.box_ids.insert(k, i);
        i
    }
}

fn shape_index(s: RenderShape) -> u8 {
    match s {
        RenderShape::Model => 0,
        RenderShape::Invisible => 1,
        RenderShape::Animated => 2,
    }
}

fn fluid_index(f: Fluid) -> u8 {
    match f {
        Fluid::None => 0,
        Fluid::Water => 1,
        Fluid::Lava => 2,
    }
}

fn encode(facts: &BlockFacts, projection: &[GameplayProjection], key: u64) -> Vec<u8> {
    // The states and projections first (they intern the tables), then the
    // header and tables in front of them.
    let mut w = Writer::default();
    for s in &facts.states {
        let ids = [
            w.string(&s.name),
            w.string(&s.item),
            w.string(&s.sounds.break_sound),
            w.string(&s.sounds.step),
            w.string(&s.sounds.place),
            w.string(&s.sounds.hit),
            w.string(&s.sounds.fall),
            w.class(&s.classes),
            w.boxes(&s.outline),
            w.boxes(&s.outline_edges),
        ];
        for i in ids {
            w.u32(i);
        }
        for b in [
            s.emission,
            s.dampening,
            u8::from(s.skylight_down),
            u8::from(s.shape_occlusion),
            u8::from(s.can_occlude),
            u8::from(s.solid_render),
            u8::from(s.requires_tool),
            u8::from(s.replaceable),
            u8::from(s.random_ticks),
            u8::from(s.block_entity),
            shape_index(s.render_shape),
            u8::from(s.full_collision),
            fluid_index(s.fluid),
            s.fluid_level,
            u8::from(s.fluid_falling),
            u8::from(s.waterlogged),
            u8::from(s.random_offset),
            u8::from(s.outline_offset),
            u8::from(s.legacy_solid),
            s.sturdy_full,
            s.map_color,
        ] {
            w.u8(b);
        }
        for f in [
            s.hardness,
            s.friction,
            s.explosion_resistance,
            s.sounds.volume,
            s.sounds.pitch,
        ] {
            w.f32(f);
        }
    }
    for p in projection {
        let ids = [
            w.string(&p.push),
            w.string(&p.place),
            w.string(&p.break_sound),
            w.string(&p.fall),
        ];
        w.u32(u32::from(p.flags));
        w.u64(p.emission);
        w.u64(p.dampening);
        w.f64(p.hardness);
        w.f64(p.volume);
        w.f64(p.pitch);
        for i in ids {
            w.u32(i);
        }
    }
    let body = std::mem::take(&mut w.out);
    w.out.extend_from_slice(MAGIC);
    w.u32(VERSION);
    w.u64(key);
    w.u32(facts.states.len() as u32);
    w.u32(projection.len() as u32);
    w.u32(facts.air.0);
    let strings = std::mem::take(&mut w.strings);
    w.u32(strings.len() as u32);
    for s in &strings {
        w.u32(s.len() as u32);
        w.out.extend_from_slice(s.as_bytes());
    }
    let classes = std::mem::take(&mut w.classes);
    w.u32(classes.len() as u32);
    for c in &classes {
        w.u32(c.len() as u32);
        for s in c.iter() {
            let i = w.string_ids[s];
            w.u32(i);
        }
    }
    let boxes = std::mem::take(&mut w.boxes);
    w.u32(boxes.len() as u32);
    for b in &boxes {
        w.u32(b.len() as u32);
        for r in b.iter() {
            for v in r {
                w.f64(*v);
            }
        }
    }
    w.out.extend_from_slice(&body);
    w.out
}

struct Reader<'a> {
    d: &'a [u8],
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        if self.d.len() < n {
            return None;
        }
        let (a, b) = self.d.split_at(n);
        self.d = b;
        Some(a)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn f64(&mut self) -> Option<f64> {
        Some(f64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn u32s<const N: usize>(&mut self) -> Option<[u32; N]> {
        let mut out = [0; N];
        for v in &mut out {
            *v = self.u32()?;
        }
        Some(out)
    }
    fn bool(&mut self) -> Option<bool> {
        Some(self.u8()? != 0)
    }
}

fn decode(
    bytes: &[u8],
    key: u64,
    expected: usize,
) -> Option<(BlockFacts, Vec<GameplayProjection>)> {
    let mut r = Reader { d: bytes };
    if r.take(8)? != MAGIC || r.u32()? != VERSION || r.u64()? != key {
        return None;
    }
    let states = r.u32()? as usize;
    let projections = r.u32()? as usize;
    let air = BlockStateId(r.u32()?);
    if states != expected || projections > states {
        return None;
    }
    let n = r.u32()? as usize;
    let mut strings: Vec<Arc<str>> = Vec::with_capacity(n.min(1 << 20));
    for _ in 0..n {
        let len = r.u32()? as usize;
        strings.push(Arc::from(std::str::from_utf8(r.take(len)?).ok()?));
    }
    let s = |i: u32| strings.get(i as usize).cloned();
    let n = r.u32()? as usize;
    let mut classes: Vec<Arc<[Arc<str>]>> = Vec::with_capacity(n.min(1 << 20));
    for _ in 0..n {
        let len = r.u32()? as usize;
        let list = (0..len).map(|_| s(r.u32()?)).collect::<Option<Vec<_>>>()?;
        classes.push(Arc::from(list));
    }
    let n = r.u32()? as usize;
    let mut boxes: Vec<Arc<[[f64; 6]]>> = Vec::with_capacity(n.min(1 << 20));
    for _ in 0..n {
        let len = r.u32()? as usize;
        let mut list = Vec::with_capacity(len.min(1 << 16));
        for _ in 0..len {
            let mut b = [0.0; 6];
            for v in &mut b {
                *v = r.f64()?;
            }
            list.push(b);
        }
        boxes.push(Arc::from(list));
    }
    let mut out = Vec::with_capacity(states);
    for _ in 0..states {
        let ids: [u32; 10] = r.u32s()?;
        let name = s(ids[0])?;
        let item = s(ids[1])?;
        let sounds = (s(ids[2])?, s(ids[3])?, s(ids[4])?, s(ids[5])?, s(ids[6])?);
        let class = classes.get(ids[7] as usize)?.clone();
        let outline = boxes.get(ids[8] as usize)?.clone();
        let outline_edges = boxes.get(ids[9] as usize)?.clone();
        let emission = r.u8()?;
        let dampening = r.u8()?;
        let skylight_down = r.bool()?;
        let shape_occlusion = r.bool()?;
        let can_occlude = r.bool()?;
        let solid_render = r.bool()?;
        let requires_tool = r.bool()?;
        let replaceable = r.bool()?;
        let random_ticks = r.bool()?;
        let block_entity = r.bool()?;
        let render_shape = match r.u8()? {
            0 => RenderShape::Model,
            1 => RenderShape::Invisible,
            _ => RenderShape::Animated,
        };
        let full_collision = r.bool()?;
        let fluid = match r.u8()? {
            1 => Fluid::Water,
            2 => Fluid::Lava,
            _ => Fluid::None,
        };
        let fluid_level = r.u8()?;
        let fluid_falling = r.bool()?;
        let waterlogged = r.bool()?;
        let random_offset = r.bool()?;
        let outline_offset = r.bool()?;
        let legacy_solid = r.bool()?;
        let sturdy_full = r.u8()?;
        let map_color = r.u8()?;
        let hardness = r.f32()?;
        let friction = r.f32()?;
        let explosion_resistance = r.f32()?;
        let volume = r.f32()?;
        let pitch = r.f32()?;
        out.push(StateFacts {
            name,
            emission,
            dampening,
            skylight_down,
            shape_occlusion,
            can_occlude,
            solid_render,
            hardness,
            requires_tool,
            replaceable,
            random_ticks,
            block_entity,
            render_shape,
            full_collision,
            friction,
            explosion_resistance,
            item,
            sounds: Sounds {
                volume,
                pitch,
                break_sound: sounds.0,
                step: sounds.1,
                place: sounds.2,
                hit: sounds.3,
                fall: sounds.4,
            },
            fluid,
            fluid_level,
            fluid_falling,
            waterlogged,
            random_offset,
            outline_offset,
            legacy_solid,
            sturdy_full,
            classes: class,
            outline,
            outline_edges,
            map_color,
        });
    }
    let mut projection = Vec::with_capacity(projections);
    for _ in 0..projections {
        let flags = u16::try_from(r.u32()?).ok()?;
        let emission = r.u64()?;
        let dampening = r.u64()?;
        let hardness = r.f64()?;
        let volume = r.f64()?;
        let pitch = r.f64()?;
        let ids: [u32; 4] = r.u32s()?;
        projection.push(GameplayProjection {
            flags,
            emission,
            dampening,
            hardness,
            push: s(ids[0])?,
            volume,
            pitch,
            place: s(ids[1])?,
            break_sound: s(ids[2])?,
            fall: s(ids[3])?,
        });
    }
    if !r.d.is_empty() {
        return None;
    }
    Some((BlockFacts { states: out, air }, projection))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(name: &str, classes: &Arc<[Arc<str>]>) -> StateFacts {
        let s = |v: &str| Arc::<str>::from(v);
        StateFacts {
            name: s(name),
            emission: 3,
            dampening: 15,
            skylight_down: true,
            shape_occlusion: false,
            can_occlude: true,
            solid_render: true,
            hardness: 1.5,
            requires_tool: true,
            replaceable: false,
            random_ticks: false,
            block_entity: false,
            render_shape: RenderShape::Animated,
            full_collision: true,
            friction: 0.6,
            explosion_resistance: 6.0,
            item: s(name),
            sounds: Sounds {
                volume: 1.0,
                pitch: 0.8,
                break_sound: s("block.stone.break"),
                step: s("block.stone.step"),
                place: s("block.stone.place"),
                hit: s("block.stone.hit"),
                fall: s("block.stone.fall"),
            },
            fluid: Fluid::Water,
            fluid_level: 8,
            fluid_falling: false,
            waterlogged: true,
            random_offset: false,
            outline_offset: true,
            legacy_solid: true,
            sturdy_full: 63,
            classes: classes.clone(),
            outline: Arc::from(vec![[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]]),
            outline_edges: Arc::from(Vec::<[f64; 6]>::new()),
            map_color: 11,
        }
    }

    #[test]
    fn facts_round_trip_and_a_stale_key_is_refused() {
        let classes: Arc<[Arc<str>]> = Arc::from(vec![Arc::from("SlabBlock"), Arc::from("Block")]);
        let facts = BlockFacts {
            states: vec![
                state("minecraft:stone_slab", &classes),
                state("minecraft:stone", &classes),
            ],
            air: BlockStateId(0),
        };
        let projection = vec![GameplayProjection {
            flags: 5,
            emission: 3,
            dampening: 15,
            hardness: 1.5,
            push: Arc::from("normal"),
            volume: 1.0,
            pitch: 0.8,
            place: Arc::from("block.stone.place"),
            break_sound: Arc::from("block.stone.break"),
            fall: Arc::from("block.stone.fall"),
        }];
        let bytes = encode(&facts, &projection, 42);
        let (back, proj) = decode(&bytes, 42, 2).expect("decodes");
        assert_eq!(format!("{:?}", back.states), format!("{:?}", facts.states));
        assert!(
            Arc::ptr_eq(&back.states[0].classes, &back.states[1].classes),
            "tables shared"
        );
        assert_eq!(
            (proj[0].flags, &*proj[0].push, proj[0].hardness),
            (5, "normal", 1.5)
        );
        assert!(decode(&bytes, 43, 2).is_none(), "another source");
        assert!(decode(&bytes, 42, 3).is_none(), "another registry");
        assert!(
            decode(&bytes[..bytes.len() - 1], 42, 2).is_none(),
            "cut short"
        );
    }
}
