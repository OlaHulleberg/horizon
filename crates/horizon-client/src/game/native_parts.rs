//! Animated parts for native mods (hook points API 42, docs/native-mods.md): a mod turns a block
//! state's model into a part model once, then sets and clears instances by its own keys. The
//! renderer draws them instanced and turns them on the GPU (`gfx/parts.rs`); nothing here runs
//! per frame but the clock, and an instance is uploaded again only when it changes.
use super::Game;
use horizon_client::{
    fast_hash::FxHashMap,
    gfx::parts::{PartInstance, PartScene},
};
use horizon_world::registry::BlockStateId;

/// The window's parts: the scene the renderer draws, where each key's instance is, the models
/// baked from block states, and the keys in each section (to relight them when it remeshes).
#[derive(Default)]
pub(super) struct NativeParts {
    pub(super) scene: PartScene,
    keys: FxHashMap<u64, (u32, u32)>,
    owners: Vec<Vec<u64>>,
    by_state: FxHashMap<u32, u32>,
    by_posed_state: FxHashMap<(u32, [u32; 16]), u32>,
    by_item: FxHashMap<(String, [u32; 16]), u32>,
    sections: FxHashMap<(i32, i32, i32), Vec<u64>>,
}

fn section_of(block: [i32; 3]) -> (i32, i32, i32) {
    (block[0] >> 4, block[1] >> 4, block[2] >> 4)
}

/// The world's stored light (`sky << 4 | block`) as the entity vertex packs it.
fn vertex_light(packed: u8) -> u32 {
    (u32::from(packed >> 4) * 16) | ((u32::from(packed & 15) * 16) << 8)
}

/// A part's light: the brightest sky and block light of its block and the six beside it. A part
/// usually reaches out of its block (a shaft's end, a wheel's paddles), and the light stored
/// inside an opaque block is none.
fn part_light(world: &horizon_client::world::World, b: [i32; 3]) -> u32 {
    let mut sky = 0u8;
    let mut block = 0u8;
    for o in [
        [0, 0, 0],
        [1, 0, 0],
        [-1, 0, 0],
        [0, 1, 0],
        [0, -1, 0],
        [0, 0, 1],
        [0, 0, -1],
    ] {
        let packed = world.light(b[0] + o[0], b[1] + o[1], b[2] + o[2]);
        sky = sky.max(packed >> 4);
        block = block.max(packed & 15);
    }
    vertex_light(sky << 4 | block)
}

impl NativeParts {
    /// Shows `instance` of `model` under `key`, moving it if the key is shown: uploaded again
    /// only if it changed.
    fn place(&mut self, key: u64, model: u32, instance: PartInstance) {
        let parts = self;
        let block = instance.block;
        if let Some(&(m, i)) = parts.keys.get(&key) {
            if m == model {
                let slot = &mut parts.scene.instances[m as usize][i as usize];
                if *slot != instance {
                    let moved = section_of(slot.block) != section_of(block);
                    if moved {
                        parts.remove(key);
                    } else {
                        *slot = instance;
                        parts.scene.revision[m as usize] += 1;
                        return;
                    }
                } else {
                    return;
                }
            } else {
                parts.remove(key);
            }
        }
        let list = &mut parts.scene.instances[model as usize];
        parts.keys.insert(key, (model, list.len() as u32));
        list.push(instance);
        parts.owners[model as usize].push(key);
        parts.scene.revision[model as usize] += 1;
        parts
            .sections
            .entry(section_of(block))
            .or_default()
            .push(key);
    }

    fn remove(&mut self, key: u64) -> Option<[i32; 3]> {
        let (model, index) = self.keys.remove(&key)?;
        let list = &mut self.scene.instances[model as usize];
        let owners = &mut self.owners[model as usize];
        let block = list[index as usize].block;
        list.swap_remove(index as usize);
        owners.swap_remove(index as usize);
        if let Some(&moved) = owners.get(index as usize) {
            self.keys.insert(moved, (model, index));
        }
        self.scene.revision[model as usize] += 1;
        if let Some(keys) = self.sections.get_mut(&section_of(block)) {
            keys.retain(|k| *k != key);
        }
        Some(block)
    }
}

/// Helpers for client mods, kept as stable as the hook points.
#[allow(dead_code)]
impl Game<'_> {
    /// The part model of block state `state`'s model (its first variant, untinted), baked the
    /// first time it is asked for: a handle for `native_part_set`. None for a state that has no
    /// model, or on a `Game` that draws nothing (a server). API 42.
    pub(super) fn native_part_model(&mut self, state: BlockStateId) -> Option<u32> {
        self.renderer.as_ref()?;
        if let Some(&model) = self.native_parts.by_state.get(&state.0) {
            return Some(model);
        }
        let mut vertices = Vec::new();
        self.tinted_block_model_vertices(
            state,
            0,
            glam::Mat4::IDENTITY,
            |_| 0,
            true,
            |_| None,
            &mut vertices,
        );
        if vertices.is_empty() {
            return None;
        }
        let model = self.native_parts.scene.add_model(vertices);
        self.native_parts.owners.push(Vec::new());
        self.native_parts.by_state.insert(state.0, model);
        Some(model)
    }

    /// [`Self::native_part_model`] under `pose`, a transform in block units (0 to 1 across the
    /// block) applied to the model before it is drawn: a partial model turned in a way its
    /// block states don't offer (Create turns a sideways belt about z). Baked once per state and
    /// pose. API 48.
    pub(super) fn native_part_model_posed(
        &mut self,
        state: BlockStateId,
        pose: glam::Mat4,
    ) -> Option<u32> {
        self.renderer.as_ref()?;
        let cache = (state.0, pose.to_cols_array().map(f32::to_bits));
        if let Some(&model) = self.native_parts.by_posed_state.get(&cache) {
            return Some(model);
        }
        let mut vertices = Vec::new();
        self.tinted_block_model_vertices(state, 0, pose, |_| 0, true, |_| None, &mut vertices);
        if vertices.is_empty() {
            return None;
        }
        let model = self.native_parts.scene.add_model(vertices);
        self.native_parts.owners.push(Vec::new());
        self.native_parts.by_posed_state.insert(cache, model);
        Some(model)
    }

    /// Shows part `model` at `block` under the mod's `key` (a key already shown moves), turning
    /// about `axis` (0 x, 1 y, 2 z) at `rpm` (positive counterclockwise looking down the axis)
    /// from `phase` degrees at time 0. Lit by the world's light at `block`, kept up to date as
    /// the block's section changes: the brightest of its block and the six beside it. API 42.
    pub(super) fn native_part_set(
        &mut self,
        key: u64,
        model: u32,
        block: [i32; 3],
        axis: u8,
        rpm: f32,
        phase: f32,
    ) {
        self.native_part_glide(key, model, block, axis, rpm, phase, [0.0; 4], [0.0; 3]);
    }

    /// [`Self::native_part_set`] for a part that also moves: it sits `offset[0..3]` blocks from
    /// `block` at clock time `offset[3]` (`native_parts_seconds`) and glides at `velocity` blocks
    /// a second from then on, on the GPU (an item on a belt), so it is uploaded again only when
    /// its motion changes. Lit at `block`. API 45.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn native_part_glide(
        &mut self,
        key: u64,
        model: u32,
        block: [i32; 3],
        axis: u8,
        rpm: f32,
        phase: f32,
        offset: [f32; 4],
        velocity: [f32; 3],
    ) {
        let parts = &mut self.native_parts;
        if model as usize >= parts.scene.models.len()
            || axis > 2
            || !rpm.is_finite()
            || !offset.iter().chain(&velocity).all(|v| v.is_finite())
        {
            return;
        }
        let instance = PartInstance {
            block,
            spin: [f32::from(axis), rpm, phase, 0.0],
            offset,
            velocity: [velocity[0], velocity[1], velocity[2], 0.0],
            light: part_light(&self.world, block),
        };
        parts.place(key, model, instance);
    }

    /// The part model of item `item` as it is drawn in a frame (`ItemDisplayContext.FIXED`,
    /// the context Create's depots and belts draw items in), placed by `pose` in the part's
    /// block (0 to 1 across it), baked the first time it is asked for. None for an unknown
    /// item, or on a `Game` that draws nothing. API 45.
    pub(super) fn native_part_item_model(&mut self, item: &str, pose: glam::Mat4) -> Option<u32> {
        self.renderer.as_ref()?;
        let cache = (item.to_owned(), pose.to_cols_array().map(f32::to_bits));
        if let Some(&model) = self.native_parts.by_item.get(&cache) {
            return Some(model);
        }
        self.res.items().get(item)?;
        let stack = horizon_client::items::ItemStack::new(item, 1);
        let state = self.resolve_item(
            &stack,
            horizon_assets::item_model::DisplayContext::Fixed,
            None,
            0,
        );
        let mut vertices = Vec::new();
        self.submit_item_world(&state, pose, 0, &mut vertices);
        if vertices.is_empty() {
            return None;
        }
        let model = self.native_parts.scene.add_model(vertices);
        self.native_parts.owners.push(Vec::new());
        self.native_parts.by_item.insert(cache, model);
        Some(model)
    }

    /// Whether item `item`'s model is lit like a block (`BakedModel.isGui3d`, a block's
    /// item): mods that lay items down draw these standing and others flat, as Create's
    /// depots and belts do. None for an unknown item or on a `Game` that draws nothing.
    /// API 47.
    pub(super) fn native_item_uses_block_light(&self, item: &str) -> Option<bool> {
        self.renderer.as_ref()?;
        self.res.items().get(item)?;
        let stack = horizon_client::items::ItemStack::new(item, 1);
        let state = self.resolve_item(
            &stack,
            horizon_assets::item_model::DisplayContext::Fixed,
            None,
            0,
        );
        Some(state.uses_block_light())
    }

    /// The parts' clock in seconds, as the GPU reads it for this frame (the time base of
    /// `native_part_glide`'s offset). API 45.
    pub(super) fn native_parts_seconds(&self) -> f32 {
        self.native_parts.scene.seconds
    }

    /// Hides the part under `key`. API 42.
    pub(super) fn native_part_clear(&mut self, key: u64) {
        self.native_parts.remove(key);
    }

    /// Hides every part of every mod (the world closed, or a mod starts over). Models stay.
    pub(super) fn native_parts_clear_all(&mut self) {
        let parts = &mut self.native_parts;
        for (model, list) in parts.scene.instances.iter_mut().enumerate() {
            if !list.is_empty() {
                list.clear();
                parts.owners[model].clear();
                parts.scene.revision[model] += 1;
            }
        }
        parts.keys.clear();
        parts.sections.clear();
    }

    /// Before a frame: the parts' clock follows the game time and the frame's partial tick, so
    /// it stops while the game is paused (hourly wrap keeps the angle precise).
    pub(super) fn native_parts_frame(&mut self, partial_tick: f32) {
        let ticks = self.world.game_time.rem_euclid(72_000) as f32;
        self.native_parts.scene.seconds = (ticks + partial_tick) / 20.0;
    }

    /// Sections whose mesh was rebuilt (their blocks or light changed): their parts take the
    /// light again, and only changed ones are uploaded.
    pub(super) fn native_parts_relight(&mut self, sections: impl Iterator<Item = (i32, i32, i32)>) {
        if self.native_parts.keys.is_empty() {
            return;
        }
        for section in sections {
            let Some(keys) = self.native_parts.sections.get(&section) else {
                continue;
            };
            for key in keys.clone() {
                let Some(&(model, index)) = self.native_parts.keys.get(&key) else {
                    continue;
                };
                let slot = &self.native_parts.scene.instances[model as usize][index as usize];
                let light = part_light(&self.world, slot.block);
                if slot.light != light {
                    self.native_parts.scene.instances[model as usize][index as usize].light = light;
                    self.native_parts.scene.revision[model as usize] += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene_with_model() -> NativeParts {
        let mut parts = NativeParts::default();
        parts.scene.add_model(vec![bytemuck::Zeroable::zeroed(); 4]);
        parts.owners.push(Vec::new());
        parts
    }

    #[test]
    fn a_glide_is_uploaded_once_until_its_motion_changes() {
        let mut parts = scene_with_model();
        let glide = PartInstance {
            block: [3, 64, 3],
            offset: [0.5, 0.1, 0.0, 12.0],
            velocity: [0.0, 0.0, 0.25, 0.0],
            ..Default::default()
        };
        parts.place(7, 0, glide);
        let shown = parts.scene.revision[0];
        parts.place(7, 0, glide);
        assert_eq!(
            parts.scene.revision[0], shown,
            "the GPU moves it, not an upload"
        );
        parts.place(
            7,
            0,
            PartInstance {
                velocity: [0.0; 4],
                ..glide
            },
        );
        assert!(parts.scene.revision[0] > shown, "a stop is uploaded");
        assert_eq!(parts.scene.instances[0].len(), 1);
    }

    fn put(parts: &mut NativeParts, key: u64, block: [i32; 3]) {
        let list = &mut parts.scene.instances[0];
        parts.keys.insert(key, (0, list.len() as u32));
        list.push(PartInstance {
            block,
            ..Default::default()
        });
        parts.owners[0].push(key);
        parts.scene.revision[0] += 1;
        parts
            .sections
            .entry(section_of(block))
            .or_default()
            .push(key);
    }

    #[test]
    fn removing_a_part_keeps_every_other_key_on_its_instance() {
        let mut parts = scene_with_model();
        for k in 0..5 {
            put(&mut parts, k, [k as i32 * 20, 0, 0]);
        }
        let before = parts.scene.revision[0];
        assert_eq!(parts.remove(1), Some([20, 0, 0]));
        assert!(
            parts.scene.revision[0] > before,
            "a removal uploads the model again"
        );
        for k in [0, 2, 3, 4] {
            let (m, i) = parts.keys[&k];
            assert_eq!(
                parts.scene.instances[m as usize][i as usize].block,
                [k as i32 * 20, 0, 0]
            );
        }
        assert_eq!(parts.remove(1), None);
        assert!(parts.sections[&(1, 0, 0)].is_empty());
    }

    #[test]
    fn light_packs_as_the_entity_vertex_reads_it() {
        // Sky 15, block 3: sky * 16 in the low byte, block * 16 in the next.
        assert_eq!(vertex_light(0xF3), 240 | 48 << 8);
    }
}
