//! A proof mod for `ClientHost::clipboard_text` and `set_clipboard_text`. With a screen open:
//! `c` copies a text, `v` reads the clipboard and copies back what it read, `t` copies how many
//! of the client ticks' reads and writes got through. Every client tick tries both.
use mod_api::{ClientHost, ClientTickPreEvent, Flow, Hooks, ScreenKeyPressedPreEvent};
use std::sync::atomic::{AtomicU32, Ordering};

static TICKS: AtomicU32 = AtomicU32::new(0);
static TICK_READS: AtomicU32 = AtomicU32::new(0);
static TICK_WRITES: AtomicU32 = AtomicU32::new(0);

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn client_tick_pre(&self, host: &mut dyn ClientHost, _: &mut ClientTickPreEvent) {
        TICKS.fetch_add(1, Ordering::Relaxed);
        if host.clipboard_text().is_some() {
            TICK_READS.fetch_add(1, Ordering::Relaxed);
        }
        if host.set_clipboard_text("written from a tick") {
            TICK_WRITES.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn screen_key_pressed_pre(&self, host: &dyn ClientHost, event: &mut ScreenKeyPressedPreEvent<'_>) -> Flow {
        match event.key {
            "key.keyboard.c" => {
                host.set_clipboard_text("copied by clipproof");
            }
            "key.keyboard.v" => {
                let read = host.clipboard_text();
                host.set_clipboard_text(&format!("clipproof read {read:?}"));
            }
            "key.keyboard.t" => {
                let [ticks, reads, writes] =
                    [&TICKS, &TICK_READS, &TICK_WRITES].map(|n| n.load(Ordering::Relaxed));
                host.set_clipboard_text(&format!(
                    "{ticks} ticks: {reads} reads and {writes} writes got through"
                ));
            }
            _ => return Flow::Continue,
        }
        Flow::Cancel
    }
}
