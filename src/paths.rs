//! Where the app keeps its data, per platform. Hand-rolled rather than pulling the
//! `dirs` crate for fifteen lines.

use std::path::PathBuf;

const APP_DIR: &str = "WhatsAppRs";

/// The app's data directory. The webview profile (message history, login session)
/// lives inside this, so it is the one directory that matters for footprint.
pub fn data_dir() -> PathBuf {
    // WHATSAPP_RS_DATA_DIR: a separate profile for a test instance, so the
    // tools can run the app beside a real, logged-in one without touching it.
    let dir = match std::env::var_os("WHATSAPP_RS_DATA_DIR") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => platform_base().join(APP_DIR),
    };
    // Best effort. If this fails the webview falls back to its own default and
    // the app still works, so it is not worth aborting over.
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[cfg(target_os = "windows")]
fn platform_base() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join("AppData").join("Local"))
}

#[cfg(target_os = "macos")]
fn platform_base() -> PathBuf {
    home().join("Library").join("Application Support")
}

#[cfg(target_os = "linux")]
fn platform_base() -> PathBuf {
    // XDG_DATA_HOME is only honoured when absolute, per the spec.
    match std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
        Some(p) if p.is_absolute() => p,
        _ => home().join(".local").join("share"),
    }
}

fn home() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(p) = std::env::var_os("USERPROFILE") {
            return PathBuf::from(p);
        }
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
