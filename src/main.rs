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
#[cfg(feature = "cef")]
mod cef_view;
#[cfg(all(feature = "firefox", target_os = "windows"))]
mod firefox_view;
#[cfg(feature = "servo")]
mod servo_view;
mod shortcut;
mod single_instance;
mod tray;
mod webview;

fn main() {
    // CEF re-runs THIS executable for its render, GPU and utility processes. Those must
    // hand control straight back to CEF and exit; a subprocess that fell through into the
    // startup below would try to take the single-instance lock and show a tray icon.
    // Nothing else may come first.
    #[cfg(feature = "cef")]
    if cef_view::intercept() {
        return;
    }

    // `--quit` asks a running instance to shut down cleanly and exits. Killing the
    // process instead skips the cookie flush, and on Servo that costs the login.
    if std::env::args().any(|a| a == "--quit") {
        let port = std::env::var("WHATSAPP_RS_INSTANCE_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok());
        single_instance::request_quit(port);
        return;
    }

    let data_dir = paths::data_dir();

    // No stored choice and no flag means ask. Cancelling means do not start:
    // silently falling through to a mode the user did not pick is how someone
    // ends up in the risky one by accident.
    let Some(mode) = mode::resolve(&data_dir) else {
        return;
    };

    // Safe mode's engine. Which one is a build-time choice, because the whole point of
    // this round is a head-to-head between two bundled engines (DECISIONS.md #15); the
    // environment variable only exists so a measurement run can force one without a
    // rebuild, and a build that does not contain that engine says so rather than
    // quietly falling back to a different one.
    let result = match mode {
        mode::Mode::Safe => run_safe(),
        mode::Mode::Light => light::run().map_err(|e| e.to_string()),
    };

    if let Err(err) = result {
        report_fatal(&format!("{err}"));
    }
}

/// Which engine safe mode runs on.
///
/// | value      | engine                          | built by                |
/// |------------|---------------------------------|-------------------------|
/// | `cef`      | bundled Chromium 152, embedded  | `--features cef`        |
/// | `firefox`  | bundled Firefox, adopted window | `--features firefox`    |
/// | `webview2` | the OS webview (Edge's engine)  | always; the fallback    |
/// | `servo`    | our Servo fork (retired, #13)   | `--features servo`      |
fn run_safe() -> Result<(), String> {
    let requested = std::env::var("WHATSAPP_RS_ENGINE").unwrap_or_default();
    match requested.as_str() {
        "cef" => {
            #[cfg(feature = "cef")]
            return cef_view::run();
            #[cfg(not(feature = "cef"))]
            return Err("this build has no bundled Chromium: rebuild with --features cef".into());
        }
        "firefox" => {
            #[cfg(all(feature = "firefox", target_os = "windows"))]
            return firefox_view::run();
            #[cfg(not(all(feature = "firefox", target_os = "windows")))]
            return Err("this build has no bundled Firefox: rebuild with --features firefox".into());
        }
        "webview2" | "os" => return webview::run().map_err(|e| e.to_string()),
        "" => {}
        other => return Err(format!("unknown engine {other:?}")),
    }

    // No explicit choice: whichever engine this binary was built with.
    #[cfg(feature = "cef")]
    return cef_view::run();
    #[cfg(all(feature = "firefox", target_os = "windows", not(feature = "cef")))]
    return firefox_view::run();
    #[cfg(all(feature = "servo", not(feature = "cef"), not(feature = "firefox")))]
    return servo_view::run().map_err(|e| e.to_string());
    #[allow(unreachable_code)]
    webview::run().map_err(|e| e.to_string())
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
