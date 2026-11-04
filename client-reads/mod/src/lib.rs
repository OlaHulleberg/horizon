//! A proof mod for `ClientHost::playing` and `ClientHost::mapping`: the HUD hook, whose host is
//! read-only, shows both.
use mod_api::{self, ClientHost, HudEvent, Hooks};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn hud(&self, host: &dyn ClientHost, event: &mut HudEvent<'_>) {
        let draw = &mut *event.draw;
        let mut lines = vec![format!("playing: {}", host.playing())];
        for name in ["key.forward", "key.jump", "key.attack"] {
            lines.push(match host.mapping(name) {
                Some(m) => format!("{name}: {} ({})", if m.down { "held" } else { "up" }, m.label.plain()),
                None => format!("{name}: none"),
            });
        }
        lines.push(format!("key.mymod.boost: {}", if host.mapping("key.mymod.boost").is_none() { "none" } else { "some" }));
        let mut y = 4.0;
        for line in &lines {
            draw.text(line, [4.0, y], 0xFFFFFF, true);
            y += 12.0;
        }
    }
}
