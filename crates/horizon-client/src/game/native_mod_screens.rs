//! Screens a native mod draws itself on the client (docs/native-mods.md, "Mod screens", API 71):
//! NeoForge's `Minecraft.setScreen` with a mod's own `Screen` (`render`, `charTyped`,
//! `mouseClicked`, `mouseScrolled`, `removed`). A mod opens one by name with
//! `native_open_mod_screen`; while it is open `native_mod_screen_draw` draws it each frame,
//! keys reach the mod through `native_input_key` (with `screen` set) and the text they type
//! through `native_mod_screen_text`, and the mouse through `native_mod_screen_mouse` and
//! `native_mod_screen_scroll`. Escape closes it unless the mod consumed the key;
//! `native_mod_screen_removed` hears it close, however it closes.
use super::{Game, Screen};
use horizon_client::clipboard::Clipboard;

/// The mod screen open on this client.
pub(super) struct ModScreen {
    pub(super) name: String,
    /// `Screen.isPauseScreen`: single player stops while it is open.
    pub(super) pause: bool,
}

#[allow(dead_code)]
impl Game<'_> {
    /// Opens the mod screen `name` over the world (`Minecraft.setScreen`), closing one already
    /// open; `pause` stops single player while it is open, as `Screen.isPauseScreen`. The
    /// pointer is freed. Client-side (the window's `Game`). API 71.
    pub(super) fn native_open_mod_screen(&mut self, name: &str, pause: bool) {
        self.native_close_mod_screen();
        self.own_screen = Some(ModScreen {
            name: name.to_owned(),
            pause,
        });
        self.screen = Screen::Mod;
        self.capture(false);
    }
    /// Closes the mod screen open, if any (`Screen.onClose`): `native_mod_screen_removed`
    /// hears it, and the game takes the pointer back. API 71.
    pub(super) fn native_close_mod_screen(&mut self) {
        let Some(open) = self.own_screen.take() else {
            return;
        };
        if self.screen == Screen::Mod {
            self.screen = Screen::Playing;
            self.capture(true);
        }
        self.native_mod_screen_removed(&open.name);
    }
    /// The name of the mod screen open on this client. API 71.
    pub(super) fn native_mod_screen(&self) -> Option<&str> {
        self.own_screen.as_ref().map(|s| s.name.as_str())
    }
    /// The system clipboard's text (`KeyboardHandler.getClipboard`), empty when it holds none.
    /// API 71.
    pub(super) fn native_clipboard_text(&mut self) -> String {
        self.clipboard.get()
    }
    /// Puts `text` on the system clipboard (`KeyboardHandler.setClipboard`). API 71.
    pub(super) fn native_set_clipboard_text(&mut self, text: &str) {
        self.clipboard.set(text);
    }
    /// Draws the open mod screen into `batch` (`Screen.render`), with the cursor in GUI pixels.
    pub(super) fn draw_mod_screen(&mut self, batch: &mut horizon_client::hud::HudBatch) {
        let Some(name) = self.own_screen.as_ref().map(|s| s.name.clone()) else {
            self.screen = Screen::Playing;
            return;
        };
        let cursor = [self.cursor.0, self.cursor.1];
        self.native_mod_screen_draw(&name, batch, cursor);
    }
    /// Text typed on the open mod screen (`Screen.charTyped`, an input method's commit, an
    /// agent's `type`).
    pub(super) fn mod_screen_type(&mut self, text: &str) {
        if let Some(name) = self.own_screen.as_ref().map(|s| s.name.clone())
            && !text.is_empty()
        {
            self.native_mod_screen_text(&name, text);
        }
    }
    /// A mouse button on the open mod screen at the cursor: 0 left, 1 right, 2 middle.
    pub(super) fn mod_screen_mouse(&mut self, button: u8, pressed: bool) {
        if let Some(name) = self.own_screen.as_ref().map(|s| s.name.clone()) {
            let at = [self.cursor.0, self.cursor.1];
            self.native_mod_screen_mouse(&name, at, button, pressed);
        }
    }
    /// Wheel steps on the open mod screen at the cursor.
    pub(super) fn mod_screen_scroll(&mut self, delta: f32) {
        if let Some(name) = self.own_screen.as_ref().map(|s| s.name.clone()) {
            let at = [self.cursor.0, self.cursor.1];
            self.native_mod_screen_scroll(&name, at, delta);
        }
    }
    /// A key on the open mod screen that no mod consumed (`native_input_key`): Escape closes
    /// it (`Screen.keyPressed`), a press or repeat types its text (`charTyped`).
    pub(super) fn mod_screen_key(&mut self, event: &winit::event::KeyEvent) {
        use winit::{
            event::ElementState,
            keyboard::{KeyCode, PhysicalKey},
        };
        if event.state != ElementState::Pressed {
            return;
        }
        if event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
            self.native_close_mod_screen();
            return;
        }
        if self.types_text()
            && let Some(text) = event.text.as_deref()
        {
            let printable: String = text.chars().filter(|c| !c.is_control()).collect();
            self.mod_screen_type(&printable);
        }
    }
    /// Whether the open mod screen pauses single player.
    pub(super) fn mod_screen_pauses(&self) -> bool {
        self.own_screen.as_ref().is_some_and(|s| s.pause)
    }
}
