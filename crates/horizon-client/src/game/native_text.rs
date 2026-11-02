//! Text in the world for native mods (hook points API 55, docs/native-mods.md): what a NeoForge
//! `BlockEntityRenderer` draws with `Font.drawInBatch` (a machine's display, a sign of its own).
//! A mod sets lines under its own key at a block and clears them; each entry is laid out with
//! the font atlas sign text uses (`block_entity_render::text_vertices`) into a part model of its
//! own (`native_parts.rs`), so it stays on the GPU and is laid out again only when the entry, or
//! the font, changes. Shown within 64 blocks of the camera (`BlockEntityRenderer.getViewDistance`)
//! in a loaded chunk: that is checked only when the camera has moved a block.
use super::{
    Game,
    native_parts::{NativeParts, part_light},
};
use glam::{DVec3, Mat4};
use horizon_client::{
    block_entity_render::{TextCall, text_vertices},
    fast_hash::FxHashMap,
    gfx::parts::PartInstance,
    hud::Rect,
};

/// One line of a mod's text.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct TextLine {
    /// From text space to the block (block units, 0 to 1 across it). Text space is font pixels,
    /// as a sign's: the line's top left at 0, x to the right, y down, a glyph cell 8 high, a
    /// string `native_text_width` wide (its GUI pixels are these). A standing sign's front text (facing south) is
    /// `translate(0.5, 0.5 + 1/3, 0.5 + 0.0467) * scale(1/96, -1/96, 1/96)`, the line moved
    /// left by half its width to centre it.
    pub(super) pose: Mat4,
    pub(super) text: String,
    /// ARGB; the alpha is ignored, as a sign's.
    pub(super) color: u32,
    /// ARGB of the outline drawn round each glyph (glowing sign text's), 0 for none.
    pub(super) outline: u32,
    /// Lit full bright (glowing sign text), else by the light at the block.
    pub(super) full_bright: bool,
}

/// One key's text: its lines and the two part models they are laid out in (lit by the block,
/// full bright), each with one instance while it is shown.
struct Entry {
    block: [i32; 3],
    lines: Vec<TextLine>,
    glyphs: u64,
    models: [Option<u32>; 2],
    shown: bool,
}

/// The mods' text by key, and where the camera was when it was last checked.
#[derive(Default)]
pub(super) struct NativeTexts {
    entries: FxHashMap<u64, Entry>,
    /// Text models of cleared entries, empty, for the next entry.
    free: Vec<u32>,
    sections: FxHashMap<(i32, i32, i32), Vec<u64>>,
    camera: Option<DVec3>,
    /// The font epoch the entries were laid out under.
    glyphs: u64,
    /// Entries laid out (for tests: unchanged ones are not).
    #[allow(dead_code)]
    laid_out: u64,
}

/// `BlockEntityRenderer.getViewDistance`.
const VIEW_DISTANCE: f64 = 64.0;
const FULL_BRIGHT: u32 = 240 | 240 << 8;

fn section_of(b: [i32; 3]) -> (i32, i32, i32) {
    (b[0] >> 4, b[1] >> 4, b[2] >> 4)
}

fn near(camera: Option<DVec3>, b: [i32; 3]) -> bool {
    camera.is_some_and(|c| {
        let centre = DVec3::new(b[0] as f64 + 0.5, b[1] as f64 + 0.5, b[2] as f64 + 0.5);
        centre.distance_squared(c) < VIEW_DISTANCE * VIEW_DISTANCE
    })
}

impl NativeTexts {
    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl NativeParts {
    /// Lays `lines` out at `block` under `key` unless that is what it shows; `glyph` and
    /// `glyphs` are the font (`Atlas::glyph_cell`, `Font::epoch`).
    fn text_set(
        &mut self,
        key: u64,
        block: [i32; 3],
        lines: &[TextLine],
        glyph: &dyn Fn(char) -> (Option<Rect>, f32),
        glyphs: u64,
    ) {
        if let Some(e) = self.texts.entries.get(&key) {
            if e.block == block && e.glyphs == glyphs && e.lines == lines {
                return;
            }
            if e.block != block {
                self.text_clear(key);
            }
        }
        let mut meshes = [Vec::new(), Vec::new()];
        for line in lines.iter().filter(|l| l.pose.is_finite()) {
            let call = TextCall {
                matrix: line.pose,
                x: 0.0,
                y: 0.0,
                text: horizon_client::bidi::visual_str(&line.text).into_owned(),
                color: line.color,
                outline: line.outline,
                light: 0,
            };
            text_vertices(glyph, &call, &mut meshes[usize::from(line.full_bright)]);
        }
        let texts = &mut self.texts;
        texts.laid_out += 1;
        if !texts.entries.contains_key(&key) {
            texts
                .sections
                .entry(section_of(block))
                .or_default()
                .push(key);
        }
        let entry = texts.entries.entry(key).or_insert_with(|| Entry {
            block,
            lines: Vec::new(),
            glyphs,
            models: [None; 2],
            shown: false,
        });
        entry.lines = lines.to_vec();
        entry.glyphs = glyphs;
        for (slot, vertices) in entry.models.iter_mut().zip(meshes) {
            match (*slot, vertices.is_empty()) {
                (Some(m), true) => {
                    self.scene.clear_model(m);
                    texts.free.push(m);
                    *slot = None;
                }
                (Some(m), false) => self.scene.set_model(m, vertices),
                (None, false) => {
                    *slot = Some(match texts.free.pop() {
                        Some(m) => {
                            self.scene.set_model(m, vertices);
                            m
                        }
                        None => {
                            self.owners.push(Vec::new());
                            let m = self.scene.add_model(vertices);
                            // Vanilla's text render type culls back faces: a sign's or a
                            // mirrored copy's text shows from its own side only.
                            self.scene.set_culled(m, true);
                            m
                        }
                    });
                }
                (None, true) => {}
            }
        }
        // Shown again below, with its instances.
        entry.shown = false;
    }

    /// Shows or hides `key`'s text, lit by `light` (block-lit lines) where it shows.
    fn text_show(&mut self, key: u64, shown: bool, light: u32) {
        let Some(e) = self.texts.entries.get_mut(&key) else {
            return;
        };
        for (bright, model) in e.models.iter().enumerate() {
            let Some(m) = *model else {
                continue;
            };
            let list = &mut self.scene.instances[m as usize];
            let instance = PartInstance {
                block: e.block,
                light: if bright == 1 { FULL_BRIGHT } else { light },
                ..Default::default()
            };
            let want = shown.then_some(instance);
            if list.first() != want.as_ref() || list.len() > 1 {
                list.clear();
                list.extend(want);
                self.scene.revision[m as usize] += 1;
            }
        }
        e.shown = shown;
    }

    fn text_clear(&mut self, key: u64) {
        let Some(e) = self.texts.entries.remove(&key) else {
            return;
        };
        for m in e.models.into_iter().flatten() {
            self.scene.clear_model(m);
            self.texts.free.push(m);
        }
        if let Some(keys) = self.texts.sections.get_mut(&section_of(e.block)) {
            keys.retain(|k| *k != key);
        }
    }

    /// Clears every mod's text (the world closed).
    pub(super) fn texts_clear_all(&mut self) {
        let keys: Vec<u64> = self.texts.entries.keys().copied().collect();
        for key in keys {
            self.text_clear(key);
        }
        self.texts.sections.clear();
        self.texts.camera = None;
    }
}

/// Helpers for client mods, kept as stable as the hook points.
#[allow(dead_code)]
impl Game<'_> {
    /// Draws `lines` at block `pos` under the mod's own `key`, in place of what the key showed
    /// (NeoForge's `BlockEntityRenderer` drawing with `Font.drawInBatch`). Laid out only when
    /// they differ from what the key shows, so a mod may call it every tick. Nothing on a `Game`
    /// that draws nothing (a server). API 55.
    pub(super) fn native_text_set(&mut self, key: u64, pos: [i32; 3], lines: &[TextLine]) {
        if self.renderer.is_none() {
            return;
        }
        let atlas = &self.gui.atlas;
        let glyph = |c: char| atlas.glyph_cell(c);
        let glyphs = atlas.font.epoch();
        let parts = &mut self.native_parts;
        parts.text_set(key, pos, lines, &glyph, glyphs);
        parts.texts.glyphs = glyphs;
        let shown = near(parts.texts.camera, pos) && self.world.loaded(pos[0], pos[2]);
        let light = part_light(&self.world, pos);
        parts.text_show(key, shown, light);
    }

    /// Takes away the text under `key`. API 55.
    pub(super) fn native_text_clear(&mut self, key: u64) {
        self.native_parts.text_clear(key);
    }

    /// Before a frame, with the camera: text is laid out again after the font changed, and
    /// shown or hidden by distance once the camera has moved a block.
    pub(super) fn native_texts_frame(&mut self, camera: DVec3) {
        let parts = &mut self.native_parts;
        if parts.texts.entries.is_empty() {
            parts.texts.camera = Some(camera);
            return;
        }
        let atlas = &self.gui.atlas;
        let glyphs = atlas.font.epoch();
        let relayout = parts.texts.glyphs != glyphs;
        let moved = parts
            .texts
            .camera
            .is_none_or(|c| c.distance_squared(camera) >= 1.0);
        if !relayout && !moved {
            return;
        }
        parts.texts.glyphs = glyphs;
        parts.texts.camera = Some(camera);
        let keys: Vec<u64> = parts.texts.entries.keys().copied().collect();
        for key in keys {
            let e = &parts.texts.entries[&key];
            let block = e.block;
            if relayout {
                let lines = e.lines.clone();
                let glyph = |c: char| atlas.glyph_cell(c);
                parts.text_set(key, block, &lines, &glyph, glyphs);
            }
            let shown = near(Some(camera), block) && self.world.loaded(block[0], block[2]);
            let e = &parts.texts.entries[&key];
            if shown != e.shown || relayout {
                parts.text_show(key, shown, part_light(&self.world, block));
            }
        }
    }

    /// Text in sections whose mesh was rebuilt takes the light again.
    pub(super) fn native_texts_relight(&mut self, section: (i32, i32, i32)) {
        let parts = &mut self.native_parts;
        let Some(keys) = parts.texts.sections.get(&section) else {
            return;
        };
        for key in keys.clone() {
            let Some(e) = parts.texts.entries.get(&key) else {
                continue;
            };
            if e.shown {
                let light = part_light(&self.world, e.block);
                parts.text_show(key, true, light);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    /// Where a glyph quad's corner lands: the pose of `line` at font pixel (`x`, `y`).
    fn at(line: &TextLine, x: f32, y: f32) -> [f32; 3] {
        line.pose.transform_point3(Vec3::new(x, y, 0.0)).to_array()
    }

    fn glyph(c: char) -> (Option<Rect>, f32) {
        let cell = (c != ' ').then_some(Rect {
            u0: 0.0,
            v0: 0.0,
            u1: 0.1,
            v1: 0.1,
            w: 8,
            h: 8,
        });
        (cell, 6.0)
    }

    fn line(text: &str, full_bright: bool) -> TextLine {
        TextLine {
            pose: Mat4::from_translation(Vec3::new(0.5, 0.75, 1.0))
                * Mat4::from_scale(Vec3::new(1.0 / 96.0, -1.0 / 96.0, 1.0 / 96.0)),
            text: text.into(),
            color: 0xFF_FFFFFF,
            outline: 0,
            full_bright,
        }
    }

    #[test]
    fn set_text_draws_glyph_quads_at_its_block_and_clear_takes_them_away() {
        let mut parts = NativeParts::default();
        let block = [3, 64, -2];
        let l = line("Hi !", false);
        parts.text_set(9, block, std::slice::from_ref(&l), &glyph, 1);
        parts.text_show(9, true, 0x30);
        let m = parts.texts.entries[&9].models[0].expect("a lit model");
        assert!(parts.texts.entries[&9].models[1].is_none());
        let vertices = &parts.scene.models[m as usize];
        // Three glyphs (the space has none), four corners each.
        assert_eq!(vertices.len(), 12);
        // The first glyph's corners: (0, 8), (8, 8), (8, 0), (0, 0) in font pixels.
        assert_eq!(vertices[0].pos, at(&l, 0.0, 8.0));
        assert_eq!(vertices[3].pos, at(&l, 0.0, 0.0));
        // The `!` after the space: two advances further on.
        assert_eq!(vertices[8].pos, at(&l, 18.0, 8.0));
        let instance = parts.scene.instances[m as usize][0];
        assert_eq!((instance.block, instance.light), (block, 0x30));
        // Drawn with its back faces culled, as vanilla's text: a mirrored copy shows from behind.
        assert!(parts.scene.culled[m as usize]);

        parts.text_clear(9);
        assert!(parts.scene.models[m as usize].is_empty());
        assert!(parts.scene.instances[m as usize].is_empty());
        assert!(parts.texts.entries.is_empty());

        // The next text takes the freed model, full bright.
        parts.text_set(10, block, &[line("A", true)], &glyph, 1);
        parts.text_show(10, true, 0);
        assert_eq!(parts.texts.entries[&10].models[1], Some(m));
        assert_eq!(parts.scene.instances[m as usize][0].light, FULL_BRIGHT);
    }

    #[test]
    fn an_unchanged_text_is_not_laid_out_again() {
        let mut parts = NativeParts::default();
        let lines = [line("Speed", false), line("Load", true)];
        parts.text_set(1, [0, 70, 0], &lines, &glyph, 1);
        parts.text_show(1, true, 0);
        let shapes = parts.scene.shapes.clone();
        let revision = parts.scene.revision.clone();
        let laid_out = parts.texts.laid_out;
        for _ in 0..3 {
            parts.text_set(1, [0, 70, 0], &lines, &glyph, 1);
            parts.text_show(1, true, 0);
        }
        assert_eq!(parts.texts.laid_out, laid_out);
        assert_eq!(parts.scene.shapes, shapes, "nothing uploaded again");
        assert_eq!(parts.scene.revision, revision);
        // A changed line, or another font, is laid out again.
        parts.text_set(1, [0, 70, 0], &[line("Speed", false)], &glyph, 1);
        assert_eq!(parts.texts.laid_out, laid_out + 1);
        parts.text_set(1, [0, 70, 0], &[line("Speed", false)], &glyph, 2);
        assert_eq!(parts.texts.laid_out, laid_out + 2);
    }

    #[test]
    fn outlined_text_sits_over_eight_outline_copies() {
        let mut parts = NativeParts::default();
        let mut l = line("A", true);
        l.outline = 0xFF_000000;
        parts.text_set(2, [0, 0, 0], &[l], &glyph, 1);
        let m = parts.texts.entries[&2].models[1].unwrap();
        assert_eq!(parts.scene.models[m as usize].len(), 9 * 4);
    }

    #[test]
    fn text_shows_within_the_view_distance() {
        let camera = Some(DVec3::new(0.5, 64.0, 0.5));
        assert!(near(camera, [0, 64, 63]));
        assert!(!near(camera, [0, 64, 64]));
        assert!(!near(None, [0, 64, 0]));
    }
}
