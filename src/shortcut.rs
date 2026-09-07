//! The Start Menu shortcut Windows needs before it will show this app's toasts.
//!
//! An unpackaged Windows program has no identity of its own. Toasts are attributed
//! by AppUserModelID, and Windows resolves that ID by looking for a Start Menu
//! shortcut whose property store carries it. The process already stamps itself
//! with `APP_ID` (see `notify.rs`); this writes the matching shortcut so the two
//! halves agree. Without it Windows drops the toast silently while every layer
//! above still reports success, which is exactly the failure the handoff called
//! "nobody has ever seen a real toast from this app".
//!
//! The file is written only when it is missing or stale (different target exe or
//! ID), so a normal launch touches nothing. It is deliberately NOT named
//! `WhatsApp.lnk`: the C# original owns that name, rewrites it whenever the target
//! differs, and is still installed on this machine.

/// Start Menu entry name. The `.lnk` extension is hidden, so this is what shows.
pub const LINK_NAME: &str = "WhatsApp Rs.lnk";

#[derive(Debug)]
pub enum Outcome {
    /// Shortcut already pointed at this exe with this ID. Nothing written.
    Current,
    /// Shortcut was missing or stale and has been (re)written.
    Written,
}

/// Make sure the shortcut exists and is current. Best effort: a failure here
/// costs toasts, not the app, so callers ignore the error.
#[cfg(target_os = "windows")]
pub fn ensure(app_id: &'static str) -> Result<Outcome, String> {
    // COM on its own thread, so the caller's apartment (wry initialises one for
    // WebView2) is never touched.
    std::thread::spawn(move || windows_impl::ensure(app_id))
        .join()
        .map_err(|_| "shortcut thread panicked".to_string())?
}

#[cfg(not(target_os = "windows"))]
pub fn ensure(_app_id: &'static str) -> Result<Outcome, String> {
    Err("no Start Menu shortcut on this platform".into())
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::{Outcome, LINK_NAME};
    use std::mem::ManuallyDrop;
    use std::path::{Path, PathBuf};

    use windows::core::{Interface, PCWSTR, PWSTR};
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::StructuredStorage::{
        PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemAlloc, CoUninitialize, IPersistFile,
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, STGM_READ,
    };
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn link_path() -> Result<PathBuf, String> {
        let appdata = std::env::var_os("APPDATA").ok_or("APPDATA is not set")?;
        Ok(PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join(LINK_NAME))
    }

    pub fn ensure(app_id: &str) -> Result<Outcome, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let link = link_path()?;

        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr.is_err() {
            return Err(format!("CoInitializeEx: {hr}"));
        }
        let result = (|| {
            if link.exists() && matches!(is_current(&link, &exe, app_id), Ok(true)) {
                return Ok(Outcome::Current);
            }
            write(&link, &exe, app_id)?;
            Ok(Outcome::Written)
        })();
        unsafe { CoUninitialize() };
        result
    }

    fn new_link() -> Result<IShellLinkW, String> {
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| format!("CoCreateInstance(ShellLink): {e}"))
    }

    /// Does the existing shortcut already point at `exe` and carry `app_id`?
    fn is_current(link: &Path, exe: &Path, app_id: &str) -> Result<bool, String> {
        let shell_link = new_link()?;
        let persist: IPersistFile = shell_link.cast().map_err(|e| e.to_string())?;
        let link_w = wide(&link.to_string_lossy());
        unsafe { persist.Load(PCWSTR(link_w.as_ptr()), STGM_READ) }
            .map_err(|e| format!("load: {e}"))?;

        let mut buf = [0u16; 1024];
        unsafe { shell_link.GetPath(&mut buf, std::ptr::null_mut(), 0) }
            .map_err(|e| format!("GetPath: {e}"))?;
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let target = String::from_utf16_lossy(&buf[..len]);
        if !target.eq_ignore_ascii_case(&exe.to_string_lossy()) {
            return Ok(false);
        }

        let store: IPropertyStore = shell_link.cast().map_err(|e| e.to_string())?;
        let value = unsafe { store.GetValue(&PKEY_AppUserModel_ID) }
            .map_err(|e| format!("GetValue: {e}"))?;
        if value.vt() != VT_LPWSTR {
            return Ok(false);
        }
        let stored = unsafe { value.Anonymous.Anonymous.Anonymous.pwszVal.to_string() }
            .map_err(|e| e.to_string())?;
        Ok(stored == app_id)
    }

    fn write(link: &Path, exe: &Path, app_id: &str) -> Result<(), String> {
        if let Some(dir) = link.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let shell_link = new_link()?;
        let exe_w = wide(&exe.to_string_lossy());
        let dir_w = wide(
            &exe.parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        let desc_w = wide("WhatsApp");
        unsafe {
            shell_link
                .SetPath(PCWSTR(exe_w.as_ptr()))
                .map_err(|e| format!("SetPath: {e}"))?;
            shell_link
                .SetWorkingDirectory(PCWSTR(dir_w.as_ptr()))
                .map_err(|e| format!("SetWorkingDirectory: {e}"))?;
            shell_link
                .SetDescription(PCWSTR(desc_w.as_ptr()))
                .map_err(|e| format!("SetDescription: {e}"))?;
            shell_link
                .SetIconLocation(PCWSTR(exe_w.as_ptr()), 0)
                .map_err(|e| format!("SetIconLocation: {e}"))?;
        }

        let store: IPropertyStore = shell_link.cast().map_err(|e| e.to_string())?;
        let value = lpwstr(app_id)?;
        unsafe {
            store
                .SetValue(&PKEY_AppUserModel_ID, &value)
                .map_err(|e| format!("SetValue: {e}"))?;
            store.Commit().map_err(|e| format!("Commit: {e}"))?;
        }

        let persist: IPersistFile = shell_link.cast().map_err(|e| e.to_string())?;
        let link_w = wide(&link.to_string_lossy());
        unsafe { persist.Save(PCWSTR(link_w.as_ptr()), true) }
            .map_err(|e| format!("Save: {e}"))?;
        Ok(())
    }

    /// A VT_LPWSTR PROPVARIANT. The shell insists on this exact type for
    /// System.AppUserModel.ID; the crate's `From<&str>` makes a BSTR, which is not it.
    /// The string is CoTaskMem-allocated because `PropVariantClear` frees it on drop.
    fn lpwstr(s: &str) -> Result<PROPVARIANT, String> {
        let w = wide(s);
        let mem = unsafe { CoTaskMemAlloc(w.len() * 2) } as *mut u16;
        if mem.is_null() {
            return Err("CoTaskMemAlloc failed".into());
        }
        unsafe { std::ptr::copy_nonoverlapping(w.as_ptr(), mem, w.len()) };
        Ok(PROPVARIANT {
            Anonymous: PROPVARIANT_0 {
                Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                    vt: VT_LPWSTR,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: PROPVARIANT_0_0_0 { pwszVal: PWSTR(mem) },
                }),
            },
        })
    }
}
