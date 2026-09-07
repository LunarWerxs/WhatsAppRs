//! WhatsApp, as a small desktop app, in one of two very different shapes.
//!
//! On first launch it asks which. The choice is remembered; `--choose` re-asks,
//! and `--safe` / `--light` set it outright.
//!
//! | mode  | how it works                                  | memory  | account risk |
//! |-------|-----------------------------------------------|---------|--------------|
//! | safe  | the OS webview, pointed at web.whatsapp.com    | ~375 MB | none         |
//! | light | speaks WhatsApp's protocol directly, no browser| ~12 MB  | ban, permanent |
//!
//! Both numbers are measured on this machine, not estimated. See `mode.rs` for
//! why the risk is real and why safe is the default.

// No console window on Windows for a GUI app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Identity Windows requires before it will render a toast for this app.
pub(crate) const APP_ID: &str = "com.lunarwerx.whatsapp-rs";

#[cfg(target_os = "windows")]
mod chat_ui;
mod chats;
#[cfg(target_os = "windows")]
mod ui_chatlist;
#[cfg(target_os = "windows")]
mod ui_messages;
#[cfg(target_os = "windows")]
mod ui_theme;
mod geometry;
mod light;
mod mode;
mod notify;
mod paths;
#[cfg(feature = "servo")]
mod servo_view;
mod shortcut;
mod single_instance;
mod tray;
mod webview;

fn main() {
    let data_dir = paths::data_dir();

    // No stored choice and no flag means ask. Cancelling means do not start:
    // silently falling through to a mode the user did not pick is how someone
    // ends up in the risky one by accident.
    let Some(mode) = mode::resolve(&data_dir) else {
        return;
    };

    // Safe mode's engine: our own Servo build when compiled with `--features servo`
    // (DECISIONS.md #8), otherwise the operating system's webview.
    let result = match mode {
        #[cfg(feature = "servo")]
        mode::Mode::Safe => servo_view::run().map_err(|e| e.to_string()),
        #[cfg(not(feature = "servo"))]
        mode::Mode::Safe => webview::run().map_err(|e| e.to_string()),
        mode::Mode::Light => light::run().map_err(|e| e.to_string()),
    };

    if let Err(err) = result {
        report_fatal(&format!("{err}"));
    }
}

/// A GUI app has no console to print to, so a failure that would otherwise be
/// silent gets a message box.
fn report_fatal(message: &str) {
    #[cfg(target_os = "windows")]
    {
        use std::iter::once;
        use windows::core::PCWSTR;
        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        let text: Vec<u16> = message.encode_utf16().chain(once(0)).collect();
        let title: Vec<u16> = "WhatsApp".encode_utf16().chain(once(0)).collect();
        unsafe {
            MessageBoxW(
                None,
                PCWSTR(text.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_OK | MB_ICONERROR,
            );
        }
    }
    #[cfg(not(target_os = "windows"))]
    eprintln!("fatal: {message}");
}
