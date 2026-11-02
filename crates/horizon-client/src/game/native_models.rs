//! Per-position block models for native mods (hook points API 59, docs/native-mods.md): what
//! NeoForge's `ModelData` gives an `IDynamicBakedModel` (`IBlockGetterExtension.getModelData`
//! read in `getQuads`). A client mod says how one position of a `dynamic_model` content block
//! is drawn, as a copycat block shows its material cut to its own shape; the chunk mesher
//! (`mesher/model_override.rs`) reads it only at such blocks.
use super::Game;
use horizon_client::{mesher::ModelOverride, world::World};

/// Sets or clears the override at `pos` on a `Game` that draws (`renders`); whether its
/// section was asked to mesh again.
fn set_override(
    world: &mut World,
    renders: bool,
    pos: [i32; 3],
    model: Option<ModelOverride>,
) -> bool {
    renders && world.set_model_override(pos, model)
}

/// Helpers for client mods, kept as stable as the hook points.
#[allow(dead_code)]
impl Game<'_> {
    /// Draws block `pos` of a content block with `properties.dynamic_model` as `model` says:
    /// `model.state`'s baked model (`faces[d]`'s for the quads facing `d`), cut to `boxes`,
    /// lit and culled as the block itself; None draws the block's own model again. Only the
    /// position's section is meshed again, and only when the override changed, so a mod may
    /// call it every tick. Kept until cleared or the level changes. Client-side (the window's
    /// `Game`); nothing on a `Game` that draws nothing (a server). API 59.
    pub(super) fn native_block_model_override(
        &mut self,
        pos: [i32; 3],
        model: Option<ModelOverride>,
    ) {
        set_override(&mut self.world, self.renderer.is_some(), pos, model);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_client::chunk::{Chunk, MIN_Y};
    use horizon_world::registry::BlockStateId;
    use std::sync::Arc;

    /// A copycat-like override: the material's state cut to a panel's box, set on a drawing
    /// `Game` and meshed again in its own section; a server keeps none.
    #[test]
    fn overrides_reach_the_world_of_a_drawing_game_only() {
        let mut world = World::new(BlockStateId(0), 0);
        world.chunks.insert((0, 0), Arc::new(Chunk::empty(0, 0)));
        let mut panel = ModelOverride::new(BlockStateId(1));
        panel.boxes.push([[0.0; 3], [1.0, 1.0, 0.125]]);
        assert!(!set_override(
            &mut world,
            false,
            [1, 64, 2],
            Some(panel.clone())
        ));
        assert!(world.model_overrides.is_empty());
        assert!(set_override(
            &mut world,
            true,
            [1, 64, 2],
            Some(panel.clone())
        ));
        assert_eq!(world.model_overrides.get(&[1, 64, 2]), Some(&panel));
        assert!(world.dirty_sections.contains(&(0, (64 - MIN_Y) >> 4, 0)));
        assert!(!set_override(&mut world, true, [1, 64, 2], Some(panel)));
        assert!(set_override(&mut world, true, [1, 64, 2], None));
        assert!(world.model_overrides.is_empty());
        let hooks = include_str!("native_hooks.rs");
        let api = hooks
            .split("pub(crate) const NATIVE_HOOKS_API: u32 = ")
            .nth(1)
            .unwrap();
        assert!(api[..api.find(';').unwrap()].parse::<u32>().unwrap() >= 59);
    }
}
