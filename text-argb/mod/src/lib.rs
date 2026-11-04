//! A proof mod for ARGB `DrawContext::text` and `BlockEntityDrawContext::text`: three HUD lines
//! in white at alpha FF, 80 and 00 (the last draws nothing), and a red label over every pig
//! (`mob_layer`, after its vanilla model), centred with `text_width`, full bright.
use mod_api::{
    self, ClientHost, Hooks, MobLayerEvent, PlayerRenderListenedEvent, RenderGuiPostEvent,
};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn render_gui_post(&self, _: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        for (i, argb) in [0xFFFF_FFFFu32, 0x80FF_FFFF, 0x00FF_FFFF].into_iter().enumerate() {
            let line = format!("text in 0x{argb:08X}");
            event.draw.text(&line, [8.0, 40.0 + 12.0 * i as f32], argb, true);
        }
    }

    fn player_render_listened(&self, _: &dyn ClientHost, event: &mut PlayerRenderListenedEvent) {
        event.listened = true;
    }

    fn mob_layer(&self, _: &dyn ClientHost, event: &mut MobLayerEvent<'_>) {
        if event.mob.kind != "minecraft:pig" {
            return;
        }
        let label = "nixie 42";
        let draw = &mut *event.draw;
        let width = draw.text_width(label);
        draw.push();
        // Above the pig, a font pixel 1/40 of a block, flipped so its y runs down as a sign's.
        draw.translate([0.0, 1.4, 0.0]);
        draw.scale([0.025, -0.025, 0.025]);
        draw.set_light(0xF0F0);
        draw.text(label, [-width / 2.0, 0.0], 0xFF2020);
        draw.pop();
    }
}
