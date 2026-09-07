//! Memory bench. Answers two questions with numbers instead of opinions:
//!
//!   1. What is the FLOOR? How much memory does the WhatsApp Web application
//!      itself require, independent of which engine renders it? Measured via the
//!      JS heap and the bytes it downloads.
//!   2. How far can the engine be pushed down with Chromium switches and
//!      WebView2's own memory-target API?
//!
//! Config comes from env so a sweep needs no rebuild:
//!   WA_ARGS   extra browser arguments (Windows only)
//!   WA_LOW    "1" to request the low memory-usage target after load
//!   WA_LABEL  label written into the report
//!   WA_SECS   seconds to run before reporting (default 30)

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use wry::{WebContext, WebViewBuilder};

const URL: &str = "https://web.whatsapp.com";

/// Reports what the PAGE costs, which is the part no engine choice can avoid.
const MEASURE_JS: &str = r#"
(function () {
  function report() {
    var r = { tag: 'page-cost' };
    try {
      if (performance && performance.memory) {
        r.jsHeapUsedMB = +(performance.memory.usedJSHeapSize / 1048576).toFixed(1);
        r.jsHeapTotalMB = +(performance.memory.totalJSHeapSize / 1048576).toFixed(1);
      } else {
        r.jsHeapUsedMB = 'performance.memory unavailable (non-Chromium)';
      }
      var res = performance.getEntriesByType('resource') || [];
      var transfer = 0, decoded = 0, scripts = 0, scriptBytes = 0;
      for (var i = 0; i < res.length; i++) {
        transfer += (res[i].transferSize || 0);
        decoded += (res[i].decodedBodySize || 0);
        if (res[i].initiatorType === 'script' || /\.js(\?|$)/.test(res[i].name)) {
          scripts++; scriptBytes += (res[i].decodedBodySize || 0);
        }
      }
      r.resourceCount = res.length;
      r.transferredMB = +(transfer / 1048576).toFixed(1);
      r.decodedMB = +(decoded / 1048576).toFixed(1);
      r.scriptCount = scripts;
      r.scriptDecodedMB = +(scriptBytes / 1048576).toFixed(1);
      r.domNodes = document.getElementsByTagName('*').length;
      r.title = document.title;
      r.href = location.href;
      r.uaSeenByPage = navigator.userAgent;
      var t = document.body ? (document.body.innerText || '') : '';
      r.bodyHead = t.slice(0, 260);
      r.hasQR = !!document.querySelector('canvas');
      r.pushesApp = /download|app store|google play|use whatsapp on your phone|not available/i.test(t);
      if (navigator.storage && navigator.storage.estimate) {
        navigator.storage.estimate().then(function (e) {
          r.storageUsedMB = +((e.usage || 0) / 1048576).toFixed(1);
          post(r);
        }).catch(function () { post(r); });
        return;
      }
    } catch (e) { r.error = String(e); }
    post(r);
  }
  function post(o) { try { window.ipc.postMessage(JSON.stringify(o)); } catch (e) {} }
  setTimeout(report, 20000);
})();
"#;

fn out_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bench-report.jsonl")
}

fn log_line(path: &PathBuf, line: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{}", line);
    }
}

fn main() -> wry::Result<()> {
    let out = out_path();
    let label = std::env::var("WA_LABEL").unwrap_or_else(|_| "unlabelled".into());
    let extra_args = std::env::var("WA_ARGS").unwrap_or_default();
    let want_low = std::env::var("WA_LOW").ok().as_deref() == Some("1");
    let secs: u64 = std::env::var("WA_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);

    log_line(
        &out,
        &format!(
            "{{\"tag\":\"config\",\"label\":{:?},\"args\":{:?},\"low\":{}}}",
            label, extra_args, want_low
        ),
    );

    let event_loop = EventLoopBuilder::new().build();
    let window = WindowBuilder::new()
        .with_title("bench")
        .with_inner_size(tao::dpi::LogicalSize::new(1180.0, 860.0))
        .build(&event_loop)
        .unwrap();

    // Each config gets its own profile so cache state cannot skew the comparison.
    let profile = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("bench-profiles")
        .join(label.replace(['/', '\\', ' '], "_"));
    let mut ctx = WebContext::new(Some(profile));

    let ipc_out = out.clone();
    let mut builder = WebViewBuilder::new_with_web_context(&mut ctx)
        .with_url(URL)
        .with_initialization_script(MEASURE_JS)
        .with_ipc_handler(move |req| log_line(&ipc_out, &req.body().to_string()));

    // WA_UA: present as a different browser. Used to answer "does WhatsApp serve a
    // lighter page to a phone", by measurement rather than assumption.
    if let Ok(ua) = std::env::var("WA_UA") {
        if !ua.is_empty() {
            log_line(&out, &format!("{{\"tag\":\"ua\",\"value\":{:?}}}", ua));
            builder = builder.with_user_agent(ua);
        }
    }

    #[cfg(target_os = "windows")]
    let builder = {
        use wry::WebViewBuilderExtWindows;
        if extra_args.is_empty() {
            builder
        } else {
            builder.with_additional_browser_args(extra_args.clone())
        }
    };

    let webview = builder.build(&window)?;

    if want_low {
        #[cfg(target_os = "windows")]
        {
            use wry::{MemoryUsageLevel, WebViewExtWindows};
            // A tray app spends most of its life hidden; this is what that should do.
            match webview.set_memory_usage_level(MemoryUsageLevel::Low) {
                Ok(()) => log_line(&out, "{\"tag\":\"memlevel\",\"applied\":\"low\"}"),
                Err(e) => log_line(
                    &out,
                    &format!("{{\"tag\":\"memlevel\",\"error\":{:?}}}", format!("{e:?}")),
                ),
            }
        }
    }
    let _webview = webview;

    // Exit on a timer so a sweep can run unattended.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(secs));
        std::process::exit(0);
    });

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::NewEvents(StartCause::Init) = event {
            println!("bench '{label}' running for {secs}s");
        }
    });
}
