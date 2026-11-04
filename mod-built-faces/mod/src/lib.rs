//! A proof mod for the mod-built `CubeFace`, `NativeBlockRim` and `NativeColoredLight`:
//! block `faceproof:box` has a block entity whose Part draws a cube above it, TNT's side on its
//! sides and only that sprite's top-left quarter on its top (`with_uv`). `visuals` puts a green
//! rim on the block at `AT`, where the proof places the box, and a red light 3 blocks east.
use mod_api::{
    self, BlockEntityPart, BlockEntitySubmitEvent, BlockPart, ClientHost, CubeFace, Hooks,
    NativeBlockRim, NativeColoredLight, Registrar, RegistrationContext, VisualsEvent,
};

/// Where the proof places the box, for the rim and the light.
const AT: [i32; 3] = [0, 100, 0];

#[derive(Clone, Default)]
struct Cube;

#[mod_api::horizon_part]
impl BlockPart for Cube {}

#[mod_api::horizon_part]
impl BlockEntityPart for Cube {
    fn block_entity_submit(&self, _: &dyn ClientHost, event: &mut BlockEntitySubmitEvent<'_>) {
        let draw = &mut *event.draw;
        draw.set_light(0xF0F0);
        let side = Some(CubeFace::new("block/tnt_side"));
        let top = Some(CubeFace::new("block/tnt_side").with_uv([0.0, 0.0, 8.0, 8.0]));
        let faces = [side, top, side, side, side, side];
        // A block-sized box, raised by the pose: a face with no UVs of its own takes them
        // from the box, as a model element's does.
        draw.push();
        draw.translate([0.0, 1.25, 0.0]);
        draw.cube([0.0; 3], [16.0; 3], faces, 0xFFFFFF);
        draw.pop();
    }
}

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        let kind = r.block_entity_type("box_cube").blocks(&["faceproof:box"]).build();
        r.block_entity_part(&kind, Cube).expect("native block entity type");
    }

    fn visuals(&self, _: &dyn ClientHost, event: &mut VisualsEvent<'_>) {
        let [x, y, z] = AT;
        event.rims.push(NativeBlockRim::new([x, y, z], 0xFF20_FF20, 3.0));
        let at = [f64::from(x) + 3.5, f64::from(y) + 0.5, f64::from(z) + 0.5];
        event.lights.push(NativeColoredLight::new(at, 0xFF2020, 3.0, 1.0));
    }
}
