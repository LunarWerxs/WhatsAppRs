//! Tray icon and its menu.
//!
//! Icons are pre-converted raw RGBA blobs embedded at compile time, so there is no
//! image-decoding dependency. Regenerate them from assets/icon.ico with the snippet
//! in README if the icon changes.

use muda::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

/// Fixed menu ids, so a handler installed before the tray exists can still tell
/// the items apart.
pub const OPEN_ID: &str = "open";
pub const RELOAD_ID: &str = "reload";
pub const MUTE_ID: &str = "mute";
pub const NOTIFY_ID: &str = "notify";
pub const AUTOSTART_ID: &str = "autostart";
pub const ABOUT_ID: &str = "about";
pub const QUIT_ID: &str = "quit";

pub const ICON_32: &[u8] = include_bytes!("../assets/icon_32.rgba");
pub const ICON_256: &[u8] = include_bytes!("../assets/icon_256.rgba");

/// What the check marks show when the menu is built.
#[derive(Clone, Copy, Debug, Default)]
pub struct State {
    pub mute_sounds: bool,
    pub notifications: bool,
    pub autostart: bool,
}

pub struct Tray {
    // Held to keep the icon alive; dropping this removes it from the tray.
    _icon: TrayIcon,
    mute: CheckMenuItem,
    notify: CheckMenuItem,
    autostart: CheckMenuItem,
}

impl Tray {
    /// The menu item toggles itself on click; these keep it honest when the host
    /// decides otherwise (a Startup shortcut that could not be written, say).
    pub fn set_mute(&self, on: bool) {
        self.mute.set_checked(on);
    }
    pub fn set_notifications(&self, on: bool) {
        self.notify.set_checked(on);
    }
    pub fn set_autostart(&self, on: bool) {
        self.autostart.set_checked(on);
    }
}

pub fn tray_image() -> Option<tray_icon::Icon> {
    tray_icon::Icon::from_rgba(ICON_32.to_vec(), 32, 32).ok()
}

pub fn window_image() -> Option<tao::window::Icon> {
    tao::window::Icon::from_rgba(ICON_256.to_vec(), 256, 256).ok()
}

pub fn build(state: State) -> Option<Tray> {
    let open = MenuItem::with_id(OPEN_ID, "Open WhatsApp", true, None);
    let reload = MenuItem::with_id(RELOAD_ID, "Reload", true, None);
    // "Mute" and not "Pause": the toasts still appear, only the sound stops. That was the
    // owner's exact request (2026-09-08) and the two are separate items for that reason.
    let mute = CheckMenuItem::with_id(MUTE_ID, "Mute sounds", true, state.mute_sounds, None);
    let notify = CheckMenuItem::with_id(
        NOTIFY_ID,
        "Show notifications",
        true,
        state.notifications,
        None,
    );
    let autostart = CheckMenuItem::with_id(
        AUTOSTART_ID,
        "Start with Windows",
        true,
        state.autostart,
        None,
    );
    let about = MenuItem::with_id(
        ABOUT_ID,
        &format!("whatsapp-rs {}", env!("CARGO_PKG_VERSION")),
        true,
        None,
    );
    let quit = MenuItem::with_id(QUIT_ID, "Quit WhatsApp", true, None);

    let menu = Menu::new();
    menu.append(&open).ok()?;
    menu.append(&reload).ok()?;
    menu.append(&PredefinedMenuItem::separator()).ok()?;
    menu.append(&mute).ok()?;
    menu.append(&notify).ok()?;
    menu.append(&autostart).ok()?;
    menu.append(&PredefinedMenuItem::separator()).ok()?;
    menu.append(&about).ok()?;
    menu.append(&quit).ok()?;

    let mut builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("WhatsApp");

    if let Some(image) = tray_image() {
        builder = builder.with_icon(image);
    }

    let icon = builder.build().ok()?;

    Some(Tray {
        _icon: icon,
        mute,
        notify,
        autostart,
    })
}
