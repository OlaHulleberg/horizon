//! A proof mod for `block_entity_view_distance`: blocks `viewproof:near` and `viewproof:far`
//! each have a block entity whose Part draws a tall red pillar above it. `far` answers a view
//! distance of 160 blocks, `near` keeps vanilla's 64.
use mod_api::{
    self, BlockEntityPart, BlockEntityRenderBoundingBoxEvent, BlockEntitySubmitEvent,
    BlockEntityViewDistanceEvent, BlockPart, ClientHost, Hooks, Registrar, RegistrationContext,
};

#[derive(Clone, Default)]
struct Pillar;

#[mod_api::horizon_part]
impl BlockPart for Pillar {}

#[mod_api::horizon_part]
impl BlockEntityPart for Pillar {
    fn block_entity_view_distance(&self, _: &dyn ClientHost, event: &mut BlockEntityViewDistanceEvent<'_>) {
        if event.block == "viewproof:far" {
            event.distance = 160.0;
        }
    }

    fn block_entity_render_bounding_box(&self, _: &dyn ClientHost, event: &mut BlockEntityRenderBoundingBoxEvent<'_>) {
        *event.bounds = [-1.0, 0.0, -1.0, 2.0, 41.0, 2.0];
    }

    fn block_entity_submit(&self, _: &dyn ClientHost, event: &mut BlockEntitySubmitEvent<'_>) {
        let draw = &mut *event.draw;
        draw.set_light(0xF0F0);
        // A pillar 3 blocks wide and 40 tall, its two faces towards north and south.
        for z in [-0.01_f32, 1.01] {
            let corners = [[-1.0, 41.0, z], [-1.0, 1.0, z], [2.0, 1.0, z], [2.0, 41.0, z]];
            draw.quad("block/stone", corners, [0.0, 0.0, 16.0, 16.0], 0xFF2020);
            let back = [corners[3], corners[2], corners[1], corners[0]];
            draw.quad("block/stone", back, [0.0, 0.0, 16.0, 16.0], 0xFF2020);
        }
    }
}

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        for block in ["near", "far"] {
            let kind = r
                .block_entity_type(&format!("{block}_pillar"))
                .blocks(&[&format!("viewproof:{block}")])
                .build();
            r.block_entity_part(&kind, Pillar).expect("native block entity type");
        }
    }
}
