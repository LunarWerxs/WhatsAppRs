//! Notifications. This is the part that silently breaks a messaging app, so it is the most
//! carefully verified code in the project, and the shortest route to it is the list of things
//! that turned out to be false.
//!
//! **The page's own report is worth nothing.** Measured on WebView2 and again on CEF: the
//! page's `onshow` fires and `getNotifications()` reports "displayed" while Windows' own
//! notification database records nothing and no toast ever appears. Every claim here was
//! checked against that database (`tools/toast-db.py`) and a screenshot, never against the
//! page.
//!
//! **Three separate things must all be true**, and each fails independently and silently:
//!
//! 1. **The permission.** CEF's Alloy runtime ignores a permission prompt by default, so an
//!    unanswered request hangs forever. `cef_view.rs` pre-grants it for WhatsApp only, with
//!    `RequestContext::set_content_setting`, and answers any prompt that still arrives.
//! 2. **Somebody has to draw it.** CEF displays no web notifications at all - Alloy turns
//!    Blink's notification support off outright - and exposes no callback carrying a
//!    notification's title and body, so the host cannot be handed one. `cef_view.rs` injects a
//!    page-side shim that forwards them and calls [`toast`] here.
//! 3. **Windows has to accept the app.** It will not render a toast for an
//!    AppUserModelID it does not know, which is what [`set_app_user_model_id`] and the Start
//!    Menu shortcut in `shortcut.rs` are for. Neither is vestigial; without them the toast is
//!    dropped with no error anywhere.
//!
//! Verified 2026-09-07 by screenshot and by Windows' database: a toast headed "WhatsApp Rs",
//! filed under `com.lunarwerx.whatsapp-rs`, from both the page and the service-worker path.

pub const APP_URL: &str = "https://web.whatsapp.com";

/// Raise a desktop notification.
///
/// The engine will not do this for us; see the module comment. Failure is deliberately
/// silent: a user who turned notifications off, or a missing notification daemon on another
/// platform, must never take down a messaging client.
pub fn toast(title: &str, body: &str) {
    let mut notification = notify_rust::Notification::new();
    notification.summary(title).body(body);

    // Windows will not render a toast whose AppUserModelID is not registered, and this is the
    // same identity `set_app_user_model_id` stamps on the process.
    #[cfg(target_os = "windows")]
    notification.app_id(crate::APP_ID);

    let _ = notification.show();
}

/// Windows requires a registered AppUserModelID before it will render a toast at all.
#[cfg(target_os = "windows")]
pub fn set_app_user_model_id(id: &str) {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
    let h = HSTRING::from(id);
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(&h);
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_app_user_model_id(_id: &str) {}
