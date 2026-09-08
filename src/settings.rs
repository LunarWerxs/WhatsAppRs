//! The tray menu's toggles, persisted. Same shape as `geometry.rs`: a few `key=value`
//! lines beside the profile, read once at start, written when a toggle changes.
//!
//! "Start with Windows" is deliberately NOT stored here. Its source of truth is whether
//! the Startup-folder shortcut exists (`shortcut.rs`), because that is what Windows
//! actually acts on; a stored flag could disagree with it and the menu would lie.

use std::path::{Path, PathBuf};

const FILE: &str = "settings.txt";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Settings {
    /// No sound from the app: WhatsApp's own message chime stays silent. Toasts still
    /// appear. See `NOTIFY_SHIM_JS` in `cef_view.rs` for how the page is told.
    pub mute_sounds: bool,
    /// Raise toasts for the page's notifications at all.
    pub notifications: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mute_sounds: false,
            notifications: true,
        }
    }
}

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join(FILE),
        }
    }

    pub fn load(&self) -> Settings {
        let mut s = Settings::default();
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return s;
        };
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let on = value.trim() == "1";
            match key.trim() {
                "mute_sounds" => s.mute_sounds = on,
                "notifications" => s.notifications = on,
                _ => {}
            }
        }
        s
    }

    /// Best effort: a failed write costs the toggle across a restart, nothing more.
    pub fn save(&self, s: Settings) {
        let body = format!(
            "mute_sounds={}\nnotifications={}\n",
            u8::from(s.mute_sounds),
            u8::from(s.notifications)
        );
        let _ = std::fs::write(&self.path, body);
    }
}
