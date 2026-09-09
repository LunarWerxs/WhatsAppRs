//! WhatsApp Web as a small desktop app, on a Chromium we ship.
//!
//! One engine, one mode, no picker. Everything else this repo once had - the operating
//! system's webview, an embedded Servo, a bundled Firefox driven as a separate process, and
//! a native-protocol client with its own hand-drawn chat window - was built, measured against
//! this, and deleted. FINDINGS.md and DECISIONS.md keep the numbers; `git log` keeps the code.
//!
//! Logged out, whole process tree, four minutes after the page loads, three runs, median.
//! This app's row is 2026-09-09, after `--in-process-gpu` was reversed (DECISIONS.md #24); the
//! rest are 2026-09-07 and that change does not touch them. The old row here read
//! "7 / 560 MB / 376 MB", which was the untrimmed configuration and had been stale since the
//! switch sweep landed - it is corrected rather than merely updated.
//!
//! | | processes | RAM | private | CPU over 4 min |
//! |---|---|---|---|---|
//! | this app | 6 | 518 MB | 350 MB | 10 s |
//! | bundled Firefox (deleted) | 10 | 1115 MB | 1040 MB | 149 s |
//! | the OS webview, Edge's engine (deleted) | 3 | 372 MB | 198 MB | 11 s |
//! | plain Chrome, what the old C# app drove | 10 | 800 MB | 571 MB | 12 s |
//!
//! Most of that is WhatsApp's own JavaScript, not the wrapper: its heap alone is 61 MB used
//! and 97 MB allocated on a logged-out login screen. Meta's own WhatsApp for Windows is
//! itself a WebView2 shell around the same page, 386 MB on disk. There is no lighter way to
//! show this website; there is only a lighter shell around it.

// No console window on Windows for a GUI app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Identity Windows requires before it will render a toast for this app.
pub(crate) const APP_ID: &str = "com.lunarwerx.whatsapp-rs";

mod cef_view;
mod engine;
mod geometry;
mod notify;
mod paths;
mod settings;
mod setup_window;
mod shortcut;
mod single_instance;
mod tray;
mod watchdog;

fn main() {
    // CEF re-runs THIS executable for its render, GPU and utility processes, marking each one
    // with a `--type=` switch. Two things follow, and the order below is exactly those two:
    //
    // 1. Every process, ours and CEF's, has to be able to FIND `libcef.dll` before it makes a
    //    CEF call. Since v0.2.0 the DLL is not beside the exe - the exe carries it compressed
    //    and unpacks it into %LOCALAPPDATA% - so `engine::prepare` comes first, always. The
    //    DLL is delay-loaded (see build.rs), which is what lets the process start without it.
    // 2. A subprocess must hand control straight back to CEF and exit. One that fell through
    //    into the startup below would take the single-instance lock and show a tray icon.
    let is_subprocess = std::env::args().any(|a| a.starts_with("--type="));

    // `--quit` asks a running instance to shut down cleanly and exits. Killing the process
    // instead skips Chromium's cookie flush, which is how a WhatsApp login gets lost. It
    // touches no CEF entry point, so it is answered before the engine is even looked for:
    // otherwise asking a running app to quit could start an unpack of its own.
    if !is_subprocess && std::env::args().any(|a| a == "--quit") {
        let port = std::env::var("WHATSAPP_RS_INSTANCE_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok());
        single_instance::request_quit(port);
        return;
    }

    let engine = match engine::prepare(is_subprocess) {
        Ok(engine) => engine,
        Err(err) => {
            // A subprocess has no window and no user to tell; CEF reports its death itself.
            if !is_subprocess {
                report_fatal(&err);
            }
            return;
        }
    };

    if cef_view::intercept() {
        return;
    }

    if let Err(err) = cef_view::run(&engine) {
        report_fatal(&err);
    }
}

/// A GUI app has no console to print to, so a failure that would otherwise be silent gets a
/// message box.
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
