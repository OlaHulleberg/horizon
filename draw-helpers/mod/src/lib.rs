//! A proof mod for the draw-context helpers: a block entity that draws a standalone model with
//! `BlockEntityDrawContext::model`, and a HUD box laid out with `DrawContext::text_width` and
//! `DrawContext::sprite_size`.
use mod_api::{
    self, BlockEntityPart, BlockEntitySubmitEvent, BlockPart, ClientHost, HudEvent, Hooks, Registrar,
    RegistrationContext, StandaloneModelsEvent,
};
use std::sync::atomic::{AtomicU8, Ordering};

const SHAFT: &str = "drawproof:block/shaft";
/// 0: no frame drew the block entity yet; 1: `model` answered false; 2: it drew.
static MODEL: AtomicU8 = AtomicU8::new(0);

#[derive(Default)]
struct Mod;

struct Shaft;

#[mod_api::horizon_part]
impl BlockPart for Shaft {}

#[mod_api::horizon_part]
impl BlockEntityPart for Shaft {
    fn block_entity_submit(&self, _: &dyn ClientHost, event: &mut BlockEntitySubmitEvent<'_>) {
        let draw = &mut *event.draw;
        let turn = (draw.game_time() % 90) as f32 * 4.0 + draw.partial() * 4.0;
        draw.push();
        draw.rotate_about([0.0, 1.0, 0.0], turn, [0.5, 0.5, 0.5]);
        let drew = draw.model(SHAFT, 0xFFFFFF);
        draw.pop();
        MODEL.store(if drew { 2 } else { 1 }, Ordering::Relaxed);
    }
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut Registrar, _: &RegistrationContext<'_>) {
        let shaft = r.block_entity_type("shaft_box").blocks(&["drawproof:shaft_box"]).build();
        r.block_entity_part(&shaft, Shaft).expect("drawproof's own block entity type");
    }

    fn standalone_models(&self, _: &RegistrationContext<'_>, event: &mut StandaloneModelsEvent<'_>) {
        event.models.push(SHAFT.into());
    }

    fn hud(&self, _: &dyn ClientHost, event: &mut HudEvent<'_>) {
        let draw = &mut *event.draw;
        let [width, _] = draw.size();
        let model = match MODEL.load(Ordering::Relaxed) {
            2 => "model: drawn",
            1 => "model: not baked",
            _ => "model: not asked yet",
        };
        let slot = "gui/sprites/container/slot";
        let [sw, sh] = draw.sprite_size(slot).unwrap_or([0.0, 0.0]);
        let lines = [
            format!("text_width: {}", draw.text_width("drawproof")),
            format!("sprite_size: {sw} x {sh}"),
            model.to_owned(),
        ];
        // Each line right-aligned 4 px from the edge, on a box exactly its measured width.
        // Below the toasts in the top-right corner.
        let mut y = 60.0;
        for line in &lines {
            let w = draw.text_width(line);
            let x = width - 4.0 - w;
            draw.fill([x - 1.0, y - 1.0, w + 2.0, 10.0], 0x90000000);
            draw.text(line, [x, y], 0xFFFFFF, false);
            y += 12.0;
        }
        // The slot sprite at its own size, right-aligned under the lines.
        draw.sprite(slot, [0.0, 0.0, sw, sh], [width - 4.0 - sw, y, sw, sh], None);
    }
}
