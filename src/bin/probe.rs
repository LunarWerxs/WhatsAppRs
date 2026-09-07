//! Capability probe. Deliberately self-contained so it stays a trustworthy
//! diagnostic, independent of whatever the app currently does.
//!
//! Answers, per platform, by measurement rather than by reading docs:
//!   1. does web.whatsapp.com load, or get an unsupported-browser page?
//!   2. which web APIs does this engine actually have?
//!   3. do notifications work, page-context AND service-worker?
//!
//! Writes newline-delimited JSON to probe-report.jsonl in the project root.
//! Run it, then read that file. See FINDINGS.md for what it has established.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

const URL: &str = "https://web.whatsapp.com";

const PROBE_JS: &str = r#"
(function () {
  // Same shim the app ships. On WebKitGTK, requestPermission() resolves "denied"
  // even when permission is genuinely granted, so report the real state instead.
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
        if (real === 'granted' || real === 'denied') { p = Promise.resolve(real); }
        else if (orig) {
          p = orig().then(function (r) {
            return Notification.permission !== 'default' ? Notification.permission : r;
          });
        } else { p = Promise.resolve(real); }
        if (typeof cb === 'function') { p.then(cb); }
        return p;
      };
    } catch (e) {}
  })();

  function post(o) { try { window.ipc.postMessage(JSON.stringify(o)); } catch (e) {} }

  function snap(tag) {
    var r = { tag: tag };
    try {
      r.ua = navigator.userAgent;
      r.title = document.title;
      r.hasNotification = (typeof Notification !== 'undefined');
      r.notificationPermission = (typeof Notification !== 'undefined') ? Notification.permission : 'NO_API';
      r.hasServiceWorker = ('serviceWorker' in navigator);
      r.swControlled = ('serviceWorker' in navigator) && !!navigator.serviceWorker.controller;
      r.hasIndexedDB = (typeof indexedDB !== 'undefined');
      r.hasSubtleCrypto = !!(window.crypto && window.crypto.subtle);
      r.hasWasm = (typeof WebAssembly !== 'undefined');
      r.hasOPFS = !!(navigator.storage && navigator.storage.getDirectory);
      r.hasSharedWorker = (typeof SharedWorker !== 'undefined');
      r.hasWebRTC = (typeof RTCPeerConnection !== 'undefined');
      r.hasMediaDevices = !!(navigator.mediaDevices && navigator.mediaDevices.getUserMedia);
      var t = document.body ? (document.body.innerText || '') : '';
      r.looksLikeLogin = !!document.querySelector('canvas');
      r.looksLikeUnsupported = /update your browser|not supported|unsupported browser|browser is not/i.test(t);
    } catch (e) { r.error = String(e); }
    post(r);
  }

  // The decisive test: fire ONLY a service-worker notification, then ask the
  // registration whether it is being DISPLAYED. A non-empty getNotifications()
  // means it really showed rather than being silently dropped.
  function swOnly() {
    var r = { tag: 'swOnly' };
    if (typeof Notification === 'undefined' || !('serviceWorker' in navigator)) {
      r.verdict = 'N/A'; post(r); return;
    }
    Notification.requestPermission().then(function (perm) {
      r.permission = perm;
      return navigator.serviceWorker.getRegistration();
    }).then(function (reg) {
      if (!reg) { r.verdict = 'no registration'; post(r); return; }
      return reg.showNotification('SW-ONLY-PROBE', { body: 'sw only', tag: 'swprobe' })
        .then(function () {
          r.showResolved = true;
          return new Promise(function (res) { setTimeout(res, 2500); });
        })
        .then(function () { return reg.getNotifications({ tag: 'swprobe' }); })
        .then(function (list) {
          r.displayedCount = list.length;
          r.verdict = list.length > 0 ? 'SW NOTIFICATION IS LIVE/DISPLAYED' : 'SW NOTIFICATION SILENTLY DROPPED';
          post(r);
        })
        .catch(function (e) { r.error = String(e); r.verdict = 'SW showNotification REJECTED'; post(r); });
    }).catch(function (e) { r.fatal = String(e); post(r); });
  }

  function pageNotify() {
    var r = { tag: 'pageNotify' };
    if (typeof Notification === 'undefined') { r.verdict = 'NO_API'; post(r); return; }
    r.permissionBefore = Notification.permission;
    Notification.requestPermission().then(function (perm) {
      r.permissionAfter = perm;
      try {
        var n = new Notification('probe-page', { body: 'page context' });
        r.pageConstructOk = true;
        setTimeout(function () { try { n.close(); } catch (e) {} }, 3000);
      } catch (e) { r.pageConstructOk = false; r.pageConstructError = String(e); }
      post(r);
    }).catch(function (e) { r.requestPermissionError = String(e); post(r); });
  }

  setTimeout(function () { snap('t5s'); }, 5000);
  setTimeout(swOnly, 9000);
  setTimeout(pageNotify, 18000);
  setTimeout(function () { snap('t28s'); }, 28000);
})();
"#;

fn out_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("probe-report.jsonl")
}

fn log_line(path: &PathBuf, line: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{}", line);
    }
}

fn main() -> wry::Result<()> {
    let out = out_path();
    let _ = std::fs::remove_file(&out);

    let event_loop = EventLoopBuilder::new().build();
    let window = WindowBuilder::new()
        .with_title("WhatsApp capability probe")
        .with_inner_size(tao::dpi::LogicalSize::new(1180.0, 860.0))
        .build(&event_loop)
        .unwrap();

    let ipc_out = out.clone();
    let builder = WebViewBuilder::new()
        .with_url(URL)
        .with_initialization_script(PROBE_JS)
        .with_ipc_handler(move |req| log_line(&ipc_out, &req.body().to_string()));

    // WA_ARGS lets the sweep test Chromium switches without a rebuild. Notably
    // --single-process, which cuts processes 7 -> 2 and RAM ~34%, but is officially
    // unsupported by Chromium, so it has to be proven not to break service workers
    // or notifications before it ships.
    #[cfg(target_os = "windows")]
    let builder = {
        use wry::WebViewBuilderExtWindows;
        match std::env::var("WA_ARGS") {
            Ok(a) if !a.is_empty() => {
                log_line(&out, &format!("{{\"tag\":\"args\",\"value\":{:?}}}", a));
                builder.with_additional_browser_args(a)
            }
            _ => builder,
        }
    };

    let webview = builder.build(&window)?;

    grant_and_observe(&webview, out.clone());

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(StartCause::Init) => {
                println!("probe running; report -> {}", out.display());
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            _ => {}
        }
    });
}

/// Grant notifications and log whatever the engine routes back to the host, so the
/// report shows not just "did it work" but "which half the host can intercept".
#[cfg(target_os = "windows")]
fn grant_and_observe(webview: &wry::WebView, out: PathBuf) {
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use webview2_com::{NotificationReceivedEventHandler, PermissionRequestedEventHandler};
    use windows::core::Interface;
    use wry::WebViewExtWindows;

    let wv = webview.webview();

    let out_perm = out.clone();
    let mut token: i64 = 0;
    unsafe {
        let _ = wv.add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(move |_wv, args| {
                if let Some(args) = args {
                    let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                    args.PermissionKind(&mut kind)?;
                    if kind == COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS {
                        args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)?;
                        log_line(
                            &out_perm,
                            "{\"tag\":\"permission\",\"kind\":\"notifications\",\"granted\":true}",
                        );
                    }
                }
                Ok(())
            })),
            &mut token,
        );
    }

    // NotificationReceived fires for PAGE notifications only, never service-worker ones.
    let out_notif = out.clone();
    if let Ok(wv24) = wv.cast::<ICoreWebView2_24>() {
        let mut t2: i64 = 0;
        unsafe {
            let _ = wv24.add_NotificationReceived(
                &NotificationReceivedEventHandler::create(Box::new(move |_wv, args| {
                    let mut title = String::new();
                    if let Some(args) = args {
                        if let Ok(n) = args.Notification() {
                            let mut t = windows::core::PWSTR::null();
                            if n.Title(&mut t).is_ok() {
                                title = t.to_string().unwrap_or_default();
                            }
                        }
                        let _ = args.SetHandled(true);
                    }
                    log_line(
                        &out_notif,
                        &format!("{{\"tag\":\"notificationReceived\",\"title\":{:?}}}", title),
                    );
                    Ok(())
                })),
                &mut t2,
            );
        }
        log_line(&out, "{\"tag\":\"wire\",\"windows\":\"permission+notificationReceived registered\"}");
    }
}

#[cfg(target_os = "linux")]
fn grant_and_observe(webview: &wry::WebView, out: PathBuf) {
    use webkit2gtk::{NotificationExt, SecurityOrigin, WebContextExt, WebViewExt};
    use wry::WebViewExtUnix;

    let wv = webview.webview();

    // The permission-request signal never fires on WebKitGTK; pre-seeding the origin
    // on the WebContext is what actually grants it.
    match wv.context() {
        Some(ctx) => {
            let origin = SecurityOrigin::for_uri(URL);
            ctx.initialize_notification_permissions(&[&origin], &[]);
            log_line(&out, "{\"tag\":\"wire\",\"linux\":\"seeded notification permission\"}");
        }
        None => log_line(&out, "{\"tag\":\"wire\",\"linux\":\"NO WebContext\"}"),
    }

    let out_notif = out.clone();
    wv.connect_show_notification(move |_wv, n| {
        let title = n.title().map(|s| s.to_string()).unwrap_or_default();
        log_line(
            &out_notif,
            &format!("{{\"tag\":\"notificationReceived\",\"title\":{:?}}}", title),
        );
        true
    });
}

#[cfg(target_os = "macos")]
fn grant_and_observe(_webview: &wry::WebView, out: PathBuf) {
    // wry's WKWebView backend does not implement the Notifications permission kind,
    // so there is nothing to grant from here yet. Saying so is the finding.
    log_line(
        &out,
        "{\"tag\":\"wire\",\"macos\":\"no notification permission API in wry's WKWebView backend\"}",
    );
}
