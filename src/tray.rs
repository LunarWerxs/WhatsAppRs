//! Tray icon and its menu.
//!
//! Icons are pre-converted raw RGBA blobs embedded at compile time, so there is no
//! image-decoding dependency. Regenerate them from assets/icon.ico with the snippet
//! in README if the icon changes.

use muda::{Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

/// Fixed menu ids, so a handler installed before the tray exists can still tell
/// the two items apart.
pub const OPEN_ID: &str = "open";
pub const QUIT_ID: &str = "quit";

pub const ICON_32: &[u8] = include_bytes!("../assets/icon_32.rgba");
pub const ICON_256: &[u8] = include_bytes!("../assets/icon_256.rgba");

pub struct Tray {
    // Held to keep the icon alive; dropping this removes it from the tray.
    _icon: TrayIcon,
    pub open_id: muda::MenuId,
    pub quit_id: muda::MenuId,
}

pub fn tray_image() -> Option<tray_icon::Icon> {
    tray_icon::Icon::from_rgba(ICON_32.to_vec(), 32, 32).ok()
}

pub fn window_image() -> Option<tao::window::Icon> {
    tao::window::Icon::from_rgba(ICON_256.to_vec(), 256, 256).ok()
}

pub fn build() -> Option<Tray> {
    let open = MenuItem::with_id(OPEN_ID, "Open WhatsApp", true, None);
    let quit = MenuItem::with_id(QUIT_ID, "Quit WhatsApp", true, None);
    let open_id = open.id().clone();
    let quit_id = quit.id().clone();

    let menu = Menu::new();
    menu.append(&open).ok()?;
    menu.append(&PredefinedMenuItem::separator()).ok()?;
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
        open_id,
        quit_id,
    })
}
