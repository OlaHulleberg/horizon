//! A proof mod for `ClientHost::play_local_sound` with a `SoundPlay`, played from a screen
//! hook (a read-only host). With a screen open: `u` plays an interface click, `p` a cow two
//! blocks east of the player (stood at 0.5 101 0.5) 40 ticks (two seconds) later, and `x` a
//! sound with a negative volume, which is refused. `horizon-ctl status` shows what the client
//! heard and what its sound engine holds queued.
use mod_api::{ClientHost, Flow, Hooks, ScreenKeyPressedPreEvent, SoundPlay, SoundSource};

/// Two blocks east of where the proof stands the player.
const EAST: [f64; 3] = [2.5, 101.0, 0.5];

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn screen_key_pressed_pre(&self, host: &dyn ClientHost, event: &mut ScreenKeyPressedPreEvent<'_>) -> Flow {
        let click = SoundPlay::ui("minecraft:ui.button.click");
        match event.key {
            "key.keyboard.u" => assert!(host.play_local_sound(&click, 0)),
            "key.keyboard.p" => {
                let cow = SoundPlay::at("minecraft:entity.cow.ambient", SoundSource::Neutral, EAST);
                assert!(host.play_local_sound(&cow, 40));
            }
            "key.keyboard.x" => assert!(!host.play_local_sound(&click.with_volume(-1.0), 0)),
            _ => return Flow::Continue,
        }
        Flow::Cancel
    }
}
