//! Notifications. This is the part that silently breaks a messaging app, so it is
//! the most carefully verified code in the project.
//!
//! Both engines DENY notifications by default, and each needs a completely different
//! host-side fix. Measured on 2026-09-06:
//!
//! - WebView2 auto-denies. `add_PermissionRequested` granting
//!   COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS flips "denied" to "granted", after
//!   which service-worker notifications genuinely display.
//! - WebKitGTK also denies, but its `permission-request` signal NEVER FIRES, because
//!   it auto-denies before asking. Connecting that signal is a dead end. The fix is
//!   `WebContext::initialize_notification_permissions` with an allowed SecurityOrigin.
//! - After the WebKitGTK fix, `Notification.permission` correctly reads "granted" and
//!   notifications work, but `Notification.requestPermission()` still resolves
//!   "denied". A site gating its notification UI on that return value would disable
//!   alerts that actually work, hence the JS shim below.

pub const APP_URL: &str = "https://web.whatsapp.com";

/// Raise a desktop notification.
///
/// Safe mode does not use this: WebView2 renders the site's own notifications
/// once the permission is granted below. Light mode has no browser to do that,
/// so it raises them itself.
///
/// Failure is deliberately silent. A missing notification daemon, or a user who
/// turned notifications off, must never take down a messaging client.
pub fn toast(title: &str, body: &str) {
    let mut notification = notify_rust::Notification::new();
    notification.summary(title).body(body);

    // Windows will not render a toast whose AppUserModelID is not registered,
    // and this is the same identity `set_app_user_model_id` stamps on the process.
    #[cfg(target_os = "windows")]
    notification.app_id(crate::APP_ID);

    let _ = notification.show();
}

/// Injected at document start on every navigation.
///
/// Reports the REAL permission state from `requestPermission()`, and never claims
/// granted when the true state is denied.
pub const PERMISSION_SHIM_JS: &str = r#"
(function () {
  if (typeof Notification === 'undefined') return;
  var orig = null;
  try {
    if (typeof Notification.requestPermission === 'function') {
      orig = Notification.requestPermission.bind(Notification);
    }
  } catch (e) { return; }
  try {
    Notification.requestPermission = function (cb) {
      var real = Notification.permission;
      var p;
      if (real === 'granted' || real === 'denied') {
        p = Promise.resolve(real);
      } else if (orig) {
        p = orig().then(function (r) {
          return Notification.permission !== 'default' ? Notification.permission : r;
        });
      } else {
        p = Promise.resolve(real);
      }
      if (typeof cb === 'function') { p.then(cb); }
      return p;
    };
  } catch (e) {}
})();
"#;

/// Grant the notification permission on whichever engine is underneath.
pub fn grant_notification_permission(webview: &wry::WebView) {
    #[cfg(target_os = "windows")]
    windows_impl::grant(webview);

    #[cfg(target_os = "linux")]
    linux_impl::grant(webview);

    // macOS: wry's WKWebView backend does not implement the Notifications
    // permission kind at all (see wry's own src/permissions.rs). A native bridge is
    // required and is not written yet, so this is deliberately a no-op rather than
    // code that pretends to work.
    #[cfg(target_os = "macos")]
    let _ = webview;
}

/// Turn the page's web notifications into real OS toasts.
///
/// Measured 2026-09-07 on Windows: with the permission granted, the page's
/// `new Notification()` fired `onshow` and the service worker's
/// `showNotification()` reported itself displayed, and Windows' own notification
/// database recorded NOTHING. WebView2 does not raise a toast for a web
/// notification on its own here. It does raise `NotificationReceived` to the host,
/// so the host draws the toast itself, the same way light mode already does.
pub fn bridge_notifications(webview: &wry::WebView) {
    #[cfg(target_os = "windows")]
    windows_impl::bridge(webview);

    // WebKitGTK shows its own notifications once the permission is granted, and
    // macOS has no permission path yet (see above), so nothing to bridge.
    #[cfg(not(target_os = "windows"))]
    let _ = webview;
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use webview2_com::{
        NotificationReceivedEventHandler, PermissionRequestedEventHandler,
        SetPermissionStateCompletedHandler,
    };
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows_core::{Interface, PCWSTR, PWSTR};
    use wry::WebViewExtWindows;

    pub fn grant(webview: &wry::WebView) {
        let wv = webview.webview();

        // Answer the page's request, should it ask.
        let mut token: i64 = 0;
        unsafe {
            let _ = wv.add_PermissionRequested(
                &PermissionRequestedEventHandler::create(Box::new(move |_wv, args| {
                    if let Some(args) = args {
                        let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                        args.PermissionKind(&mut kind)?;
                        if kind == COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS {
                            args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)?;
                        }
                    }
                    Ok(())
                })),
                &mut token,
            );
        }

        // And grant it up front on the profile, so `Notification.permission` reads
        // "granted" from the first paint. Measured 2026-09-07: with only the handler
        // above it read "default" until the page asked, and WhatsApp Web keeps its
        // "turn on desktop notifications" banner up until then.
        let Ok(wv13) = wv.cast::<ICoreWebView2_13>() else {
            return;
        };
        let Ok(profile) = (unsafe { wv13.Profile() }) else {
            return;
        };
        let Ok(profile4) = profile.cast::<ICoreWebView2Profile4>() else {
            return;
        };
        let origin: Vec<u16> = super::APP_URL
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            let _ = profile4.SetPermissionState(
                COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS,
                PCWSTR(origin.as_ptr()),
                COREWEBVIEW2_PERMISSION_STATE_ALLOW,
                &SetPermissionStateCompletedHandler::create(Box::new(|_| Ok(()))),
            );
        }
    }

    pub fn bridge(webview: &wry::WebView) {
        let wv = webview.webview();
        let Ok(wv24) = wv.cast::<ICoreWebView2_24>() else {
            return;
        };
        let mut token: i64 = 0;
        unsafe {
            let _ = wv24.add_NotificationReceived(
                &NotificationReceivedEventHandler::create(Box::new(|_wv, args| {
                    let Some(args) = args else {
                        return Ok(());
                    };
                    let notification = args.Notification()?;
                    let title = string_field(|out| notification.Title(out));
                    let body = string_field(|out| notification.Body(out));
                    // We draw it, so the runtime must not; and the page's `onshow`
                    // waits on ReportShown.
                    args.SetHandled(true)?;
                    super::toast(&title, &body);
                    let _ = notification.ReportShown();
                    Ok(())
                })),
                &mut token,
            );
        }
    }

    /// Read a CoTaskMem-allocated string out-parameter and free it.
    fn string_field(get: impl FnOnce(*mut PWSTR) -> windows_core::Result<()>) -> String {
        let mut p = PWSTR::null();
        if get(&mut p).is_err() || p.is_null() {
            return String::new();
        }
        let s = unsafe { p.to_string() }.unwrap_or_default();
        unsafe { CoTaskMemFree(Some(p.as_ptr() as *const _)) };
        s
    }
}

#[cfg(target_os = "linux")]
mod linux_impl {
    use webkit2gtk::{SecurityOrigin, WebContextExt, WebViewExt};
    use wry::WebViewExtUnix;

    pub fn grant(webview: &wry::WebView) {
        let wv = webview.webview();
        if let Some(ctx) = wv.context() {
            let origin = SecurityOrigin::for_uri(super::APP_URL);
            ctx.initialize_notification_permissions(&[&origin], &[]);
        }
    }
}

/// Windows requires a registered AppUserModelID before it will render a toast at all.
/// This is why the C# original's Shortcut.cs and TrayPromotion.cs are not vestigial
/// cruft and must be kept.
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
