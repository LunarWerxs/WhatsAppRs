//! Embeds the Windows application manifest.
//!
//! Without it the process never starts. The first-run picker (`mode.rs`) calls
//! `TaskDialogIndirect`, which only exists in Common Controls version 6, and
//! Windows only loads version 6 when the executable's manifest asks for it. An
//! unmanifested exe gets version 5, the import cannot be resolved, and the loader
//! kills the process with STATUS_ENTRYPOINT_NOT_FOUND (0xC0000139) before `main`
//! runs. Measured 2026-09-07: exit code -1073741511, 1.4 s, no window.
//!
//! The same manifest also declares per-monitor DPI awareness, so the window and
//! the picker render sharp on scaled displays instead of being bitmap-stretched.

fn main() {
    #[cfg(target_os = "windows")]
    {
        use embed_manifest::{embed_manifest, new_manifest};
        // Everything the picker needs is in the crate's defaults: the comctl32 v6
        // dependency, PerMonitorV2 DPI awareness, UTF-8 code page, long paths, and
        // the supported-OS list, run as the invoking user.
        embed_manifest(new_manifest("Lunarwerx.WhatsAppRs"))
            .expect("failed to embed the Windows manifest");

        // The icon Explorer, the taskbar and the Start Menu show, plus the version block
        // in the file's Properties. Both come from a compiled .rc, so this needs rc.exe.
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "WhatsApp Rs");
        res.set("FileDescription", "WhatsApp Web as a small tray app");
        res.set("LegalCopyright", "MIT License");
        res.compile().expect("failed to compile the Windows resource (is rc.exe on PATH?)");
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon.ico");
}
