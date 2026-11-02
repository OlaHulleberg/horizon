//! Per-position model overrides for `dynamic_model` blocks: what NeoForge's
//! `ModelData` gives an `IDynamicBakedModel`. A native mod says, for one position,
//! which state's baked model to draw there and the boxes to cut it to (a copycat
//! drawing its material in its own shape); the mesher looks a position up only
//! for states flagged `dynamic_model`.
use super::BakedFace;
use crate::fast_hash::FxHashMap;
use horizon_core::geometry::Direction;
use horizon_world::registry::BlockStateId;

/// What the chunk mesher draws at one position of a `dynamic_model` block.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelOverride {
    /// The state whose baked model is drawn (a copycat's material).
    pub state: BlockStateId,
    /// By `Direction` ordinal, a state whose model gives the quads facing that way
    /// instead of `state`'s.
    pub faces: [Option<BlockStateId>; 6],
    /// Boxes in block units (`[min, max]`, 0 to 1) the model is cut to: each quad on
    /// the block's side moves to the box's side and is clipped to it, inner quads are
    /// clipped. Empty draws the whole model.
    pub boxes: Vec<[[f32; 3]; 2]>,
}
impl ModelOverride {
    /// `state`'s whole model.
    pub fn new(state: BlockStateId) -> Self {
        Self {
            state,
            faces: [None; 6],
            boxes: Vec::new(),
        }
    }
    /// The state whose model gives the quads facing `d`.
    pub fn face_state(&self, d: Direction) -> BlockStateId {
        self.faces[d as usize].unwrap_or(self.state)
    }
}
/// The overrides of a level, by block position.
pub type ModelOverrides = FxHashMap<[i32; 3], ModelOverride>;

const E: f32 = 1.0e-4;

/// `face` cut to box `b`, or None when nothing of it is left. An axis-aligned
/// rectangle on the block's side is moved to the box's side in that direction and
/// clipped to the box across it, its UVs following; it keeps its cull face only
/// while the box reaches the block's side. Any other quad is kept only whole
/// inside the box.
pub(super) fn cut(face: &BakedFace, b: &[[f32; 3]; 2]) -> Option<BakedFace> {
    let q = &face.quad;
    // Direction order: down, up, north, south, west, east.
    let a = [1, 2, 0][q.facing as usize / 2];
    let (u, w) = ((a + 1) % 3, (a + 2) % 3);
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &q.positions {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    let at = |v: f32, x: f32| (v - x).abs() < E;
    let corner = |p: &[f32; 3]| {
        let side = |i: usize| (at(p[i], lo[i]) || at(p[i], hi[i])).then_some(at(p[i], hi[i]));
        Some((side(u)?, side(w)?))
    };
    let rectangle = at(lo[a], hi[a]) && hi[u] - lo[u] > E && hi[w] - lo[w] > E;
    let corners: Option<Vec<_>> = q.positions.iter().map(corner).collect();
    let Some(corners) = corners.filter(|_| rectangle) else {
        let inside = q
            .positions
            .iter()
            .all(|p| (0..3).all(|i| p[i] >= b[0][i] - E && p[i] <= b[1][i] + E));
        return inside.then(|| face.clone());
    };
    let positive = q.facing as usize % 2 == 1;
    let edge = if positive { 1.0 } else { 0.0 };
    let on_side = at(lo[a], edge);
    let plane = match (on_side, positive) {
        (true, true) => b[1][a],
        (true, false) => b[0][a],
        _ if lo[a] >= b[0][a] - E && lo[a] <= b[1][a] + E => lo[a],
        _ => return None,
    };
    let (u0, u1) = (lo[u].max(b[0][u]), hi[u].min(b[1][u]));
    let (w0, w1) = (lo[w].max(b[0][w]), hi[w].min(b[1][w]));
    if u1 - u0 <= E || w1 - w0 <= E {
        return None;
    }
    // The UV at each of the rectangle's corners, by (high u, high w).
    let mut uv = [[0f32; 2]; 4];
    for (i, &(cu, cw)) in corners.iter().enumerate() {
        uv[usize::from(cu) | usize::from(cw) << 1] = q.uv[i];
    }
    let lerp =
        |x: [f32; 2], y: [f32; 2], t: f32| [x[0] + (y[0] - x[0]) * t, x[1] + (y[1] - x[1]) * t];
    let mut out = face.clone();
    for (i, &(cu, cw)) in corners.iter().enumerate() {
        let p = &mut out.quad.positions[i];
        p[a] = plane;
        p[u] = if cu { u1 } else { u0 };
        p[w] = if cw { w1 } else { w0 };
        let (s, t) = (
            (p[u] - lo[u]) / (hi[u] - lo[u]),
            (p[w] - lo[w]) / (hi[w] - lo[w]),
        );
        out.quad.uv[i] = lerp(lerp(uv[0], uv[1], s), lerp(uv[2], uv[3], s), t);
    }
    if on_side && !at(plane, edge) {
        out.quad.cull = None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_assets::{bake::ModelQuad, model::Direction};

    fn face(positions: [[f32; 3]; 4], facing: Direction, cull: Option<Direction>) -> BakedFace {
        let quad = ModelQuad {
            positions,
            uv: [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
            facing,
            cull,
            material: horizon_assets::model::Material {
                sprite: String::new(),
                force_translucent: false,
            },
            tint_index: -1,
            shade_direction_override: None,
            light_emission: 0,
        };
        BakedFace {
            quad,
            layer: 7,
            opaque: true,
            translucent: false,
        }
    }
    const UP: [[f32; 3]; 4] = [
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 1.0],
        [1.0, 1.0, 1.0],
        [1.0, 1.0, 0.0],
    ];
    const EAST: [[f32; 3]; 4] = [
        [1.0, 1.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
    ];

    /// A slab's box: the top moves down to the box and loses its cull face, a side
    /// is clipped to the lower half with its UVs, the bottom keeps its own.
    #[test]
    fn faces_on_the_side_move_to_the_box_and_clip_with_their_uvs() {
        let slab = [[0.0, 0.0, 0.0], [1.0, 0.5, 1.0]];
        let top = cut(&face(UP, Direction::Up, Some(Direction::Up)), &slab).unwrap();
        assert!(top.quad.positions.iter().all(|p| p[1] == 0.5));
        assert_eq!(top.quad.cull, None);
        assert_eq!(top.quad.uv, face(UP, Direction::Up, None).quad.uv);
        let east = cut(&face(EAST, Direction::East, Some(Direction::East)), &slab).unwrap();
        assert_eq!(east.quad.cull, Some(Direction::East));
        assert_eq!(east.layer, 7);
        let ys: Vec<f32> = east.quad.positions.iter().map(|p| p[1]).collect();
        assert_eq!(ys, [0.5, 0.0, 0.0, 0.5]);
        // y is the quad's u axis there (u = y, w = z): the top corners' UVs meet the middle.
        assert_eq!(east.quad.uv[0], [0.0, 0.5]);
        assert_eq!(east.quad.uv[1], [0.0, 1.0]);
        assert_eq!(east.quad.uv[3], [1.0, 0.5]);
    }

    /// A quad outside the box, or inner and not crossing it, is dropped; a quad that
    /// isn't an axis-aligned rectangle is kept only whole inside.
    #[test]
    fn quads_outside_the_box_are_dropped() {
        let panel = [[0.0, 0.0, 0.0], [0.125, 1.0, 1.0]];
        let mut inner = EAST;
        for p in &mut inner {
            p[0] = 0.5;
        }
        assert!(cut(&face(inner, Direction::East, None), &panel).is_none());
        let east = cut(&face(EAST, Direction::East, Some(Direction::East)), &panel).unwrap();
        assert!(east.quad.positions.iter().all(|p| p[0] == 0.125));
        assert_eq!(east.quad.cull, None);
        let mut tilted = EAST;
        tilted[0][0] = 0.9;
        assert!(cut(&face(tilted, Direction::East, None), &panel).is_none());
        let whole = [[0.0; 3], [1.0; 3]];
        assert!(cut(&face(tilted, Direction::East, None), &whole).is_some());
    }

    /// In a section, an override draws the material's faces cut to the box in place of
    /// the block's own; clearing it draws the block again; a block without
    /// `dynamic_model` never reads the overrides.
    #[test]
    fn the_mesher_draws_an_override_in_place_of_the_block() {
        use crate::mesher::*;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/horizon-client/26.3");
        if !root.join("horizon-resources.json").exists() {
            return;
        }
        let assets = Assets::load(&root).unwrap();
        let render = assets.reload(&root).unwrap();
        let facts = BlockFacts::load(&root, assets.registry).unwrap();
        let visibility = crate::models::load_visibility(&root, assets.registry).unwrap();
        let mut tables = MeshTables::new(&assets, &render, &facts, visibility).unwrap();
        let biomes = Biomes::load(&root).unwrap();
        let state = |name| assets.registry.default_state(name).unwrap();
        let (glass, stone) = (state("minecraft:glass"), state("minecraft:stone"));
        // As a content block with `properties.dynamic_model`.
        tables.info[glass.0 as usize].dynamic = true;
        let y = MIN_Y + 20;
        let mut chunk = Chunk::empty(0, 0);
        chunk.set(5, y, 5, glass.0);
        chunk.set(9, y, 5, stone.0);
        let mut region = [[None; 3]; 3];
        region[1][1] = Some(&chunk);
        let mesh = |overrides: Option<&ModelOverrides>| {
            let mut mesher = Mesher {
                nether_shading: false,
                assets: &assets,
                render: &render,
                tables: &tables,
                biomes: &biomes,
                visibility: tables.visibility.clone(),
                zoom: BiomeZoom::new(0),
                blend_radius: 2,
                smooth_lighting: true,
                cutout_leaves: true,
                overrides,
            };
            mesher.mesh(&region, (y - MIN_Y) >> 4)
        };
        // The sprite layers and highest y of the quads at column `x` (z 5).
        let block = |mesh: &SectionMesh, x: f32| {
            let mut layers = std::collections::BTreeSet::new();
            let (mut count, mut top) = (0, f32::MIN);
            for v in mesh.solid.iter().chain(&mesh.translucent) {
                if (x..=x + 1.0).contains(&v.pos[0]) && (5.0..=6.0).contains(&v.pos[2]) {
                    layers.insert(u32::from(v.layer & !OPAQUE_LAYER));
                    top = top.max(v.pos[1]);
                    count += 1;
                }
            }
            (
                layers.into_iter().collect::<Vec<_>>(),
                count,
                top - (y & 15) as f32,
            )
        };
        let sprite = |s| vec![render.model_catalog.particle(s).unwrap()];
        let plain = mesh(None);
        assert_eq!(block(&plain, 5.0), (sprite(glass), 24, 1.0));
        assert_eq!(block(&plain, 9.0), (sprite(stone), 24, 1.0));
        let mut overrides = ModelOverrides::default();
        let mut slab = ModelOverride::new(stone);
        slab.boxes.push([[0.0; 3], [1.0, 0.5, 1.0]]);
        overrides.insert([5, y, 5], slab);
        // Stone has no `dynamic_model`: an entry at its position is never read.
        overrides.insert([9, y, 5], ModelOverride::new(glass));
        let drawn = mesh(Some(&overrides));
        assert_eq!(block(&drawn, 5.0), (sprite(stone), 24, 0.5));
        assert_eq!(block(&drawn, 9.0), (sprite(stone), 24, 1.0));
        overrides.remove(&[5, y, 5]);
        let cleared = mesh(Some(&overrides));
        assert_eq!(block(&cleared, 5.0), (sprite(glass), 24, 1.0));
        assert_eq!(cleared.solid.len(), plain.solid.len());
        assert_eq!(cleared.translucent.len(), plain.translucent.len());
    }
}
