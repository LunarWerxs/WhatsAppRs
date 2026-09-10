//! Safe mode on a bundled Chromium: the Chromium Embedded Framework, in our own window.
//!
//! # ⛔⛔ THE ENGINE IS OS-AGNOSTIC OR IT IS NOT A CANDIDATE ⛔⛔
//!
//! > **"I will never, ever, ever, ever goddamn use a motherfucking browser that is not OS
//! > agnostic."** - Michael, 2026-09-09 (DECISIONS.md #26, and the banner at the top of it)
//!
//! **The same engine on Windows, macOS and Linux, or it is out.** Not one engine per platform.
//! The same one. This is WHY this file bundles CEF and why 271 MB of `libcef.dll` is an accepted
//! cost rather than a problem to be solved.
//!
//! **⛔ WebView2 / Microsoft Edge is permanently closed. Do not raise it.** Microsoft ships
//! WebView2 for Windows only: no Linux build, no macOS build. It fails the rule before its size,
//! its user agent or its identity is worth a sentence. The same goes for "just use the system
//! webview", which is WebView2 plus WKWebView plus WebKitGTK, three engines pretending to be a
//! strategy.
//!
//! The size argument that keeps motivating this has been searched to the end and closed: no
//! smaller engine that can render web.whatsapp.com exists, CEF's own maintainer states the size
//! is inherent to Chromium, and every wrapper bundling a real Chromium lands in the same band.
//!
//! This is the first of the two candidates ruled in by DECISIONS.md #15. The engine
//! ships with the app rather than being the operating system's (which is Edge here, and
//! rejected) or a browser the user has installed. CEF gives a real child HWND, so the
//! tray, close-to-tray, single-instance and geometry code carry over untouched: the
//! browser is a child window of the window we already own, exactly like WebView2 was.
//!
//! Three things about CEF that shape this file, each of which will bite anyone who
//! forgets them:
//!
//! 1. **CEF re-runs THIS EXE for its render, GPU and utility processes.** `intercept()`
//!    must be the first thing `main` does. A subprocess that falls through into our
//!    startup would show the tray icon, take the single-instance lock, and hang.
//! 2. **`multi_threaded_message_loop` is what lets tao keep its own event loop.** CEF
//!    then runs its UI thread by itself, so we never call `run_message_loop`, and every
//!    CEF callback in this file arrives on a thread that is not ours.
//! 3. **The browser must be created on CEF's UI thread, not ours.** Request contexts and
//!    browser creation both belong there, and calling into them from the thread that
//!    called `initialize` kills the process with an access violation and nothing in the
//!    log. So the order is: our window first, its handle parked in a static, `initialize`,
//!    and then `on_context_initialized` - which CEF runs on its own thread - creates the
//!    browser as a child of that handle.

use crate::{geometry, notify, paths, settings, shortcut, single_instance, tray, watchdog, APP_ID};

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cef::rc::Rc as _;
use cef::*;

use tao::dpi::{LogicalPosition, LogicalSize};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::windows::WindowExtWindows;
use tao::window::{Window, WindowBuilder};

const DEFAULT_W: f64 = 1180.0;
const DEFAULT_H: f64 = 860.0;
const TICK: Duration = Duration::from_millis(1500);

/// The browser's own child HWND, so a resize of our window can resize it. Stored as a
/// plain integer because it is written from CEF's UI thread and read from ours.
static BROWSER_HWND: AtomicIsize = AtomicIsize::new(0);
/// Our window, handed across to the CEF UI thread.
///
/// The browser cannot be created on our thread. `CefRequestContext` and browser creation
/// both belong to CEF's UI thread, and calling into them from the thread that called
/// `initialize` crashes the process with an access violation and no log line - which is
/// exactly what the first version of this file did. So the window is created first, its
/// handle is parked here, and `on_context_initialized` (which CEF runs on its own UI
/// thread) picks it up.
static PARENT_HWND: AtomicIsize = AtomicIsize::new(0);
static INIT_W: AtomicI32 = AtomicI32::new(DEFAULT_W as i32);
static INIT_H: AtomicI32 = AtomicI32::new(DEFAULT_H as i32);

/// The tray toggles, mirrored where CEF's threads can read them. Written on our thread
/// from the menu; read on the UI thread (`push_mute`) and in the console callback.
static MUTED: AtomicBool = AtomicBool::new(false);
static NOTIFICATIONS: AtomicBool = AtomicBool::new(true);

/// The one browser, kept so a menu click on our thread can reach it. A `Browser` may only
/// be *used* on CEF's UI thread; the menu never uses it, it posts a `UiTask` there, which
/// is the only reader. The `Send` claim covers exactly that hand-off and nothing else.
struct UiBrowser(Browser);
unsafe impl Send for UiBrowser {}
static BROWSER: Mutex<Option<UiBrowser>> = Mutex::new(None);

/// Where the "About" item points.
const REPO_URL: &str = "https://github.com/LunarWerxs/WhatsAppRs";

/// Must be the first statement in `main`.
///
/// CEF launches its own child processes by re-executing our executable with a `--type=`
/// switch. `execute_process` returns -1 in the browser process and >= 0 in every other,
/// and the others must return from `main` immediately.
pub fn intercept() -> bool {
    // Pins the API version. Without it the loader resolves CEF's experimental
    // (unversioned) entry points, which are explicitly not compatible across builds.
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let args = args::Args::new();
    // The app object has to be passed HERE, not only to `initialize`. The render process
    // is a separate process that never reaches `initialize`, so a handler installed only
    // there does not exist where the page's JavaScript lives - which is where the
    // notification shim has to be injected. The cefsimple sample passes None and
    // therefore has no render-process handler at all.
    let mut app = HostApp::new(RefCell::new(None));
    execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    ) >= 0
}

#[derive(Debug)]
enum UserEvent {
    TrayClick,
    MenuOpen,
    MenuReload,
    MenuMute,
    MenuNotify,
    MenuAutostart,
    MenuAbout,
    MenuQuit,
}

pub(crate) fn run(engine: &crate::engine::Engine) -> Result<(), String> {
    let listener = match single_instance::acquire() {
        single_instance::Instance::Second => return Ok(()),
        single_instance::Instance::First(l) => l,
    };

    notify::set_app_user_model_id(APP_ID);
    let _ = shortcut::ensure(APP_ID);

    let data_dir = paths::data_dir();
    // Before anything opens the log, and only here in the browser process: clears last
    // run's log files, then watches this run's from the tick below. See watchdog.rs, and
    // the 244 GB it exists because of.
    let mut watchdog = watchdog::Watchdog::arm(&data_dir);
    let mut store = geometry::Store::new(&data_dir);
    let saved = store.load();

    let prefs = settings::Store::new(&data_dir);
    let mut current = prefs.load();
    MUTED.store(current.mute_sounds, Ordering::SeqCst);
    NOTIFICATIONS.store(current.notifications, Ordering::SeqCst);

    // `--minimized` is what the Startup-folder shortcut passes: sit in the tray at logon
    // instead of opening the chat window over whatever the user was about to do.
    let start_hidden = std::env::args().any(|a| a == "--minimized");

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();

    let mut window_builder = WindowBuilder::new()
        .with_title("WhatsApp")
        .with_visible(!start_hidden)
        .with_inner_size(LogicalSize::new(DEFAULT_W, DEFAULT_H));
    if let Some(icon) = tray::window_image() {
        window_builder = window_builder.with_window_icon(Some(icon));
    }
    if let Some(p) = saved {
        window_builder = window_builder
            .with_inner_size(LogicalSize::new(p.w as f64, p.h as f64))
            .with_position(LogicalPosition::new(p.x as f64, p.y as f64));
    }
    let window = window_builder
        .build(&event_loop)
        .map_err(|e| format!("window: {e}"))?;
    if saved.map(|p| p.maximized).unwrap_or(false) {
        window.set_maximized(true);
    }

    let args = args::Args::new();

    let cef_dir = engine.dir.clone();
    // Which engine this run is using, and whether this run is the one that unpacked it.
    // It is the one line that tells a test whether it exercised first run or second.
    eprintln!(
        "[whatsapp-rs] engine {} ({})",
        cef_dir.display(),
        if engine.beside_exe {
            "beside the exe"
        } else if engine.extracted {
            "unpacked by this run"
        } else {
            "already unpacked"
        }
    );
    let mut settings = Settings {
        // CEF's own sandbox needs a separate bootstrap executable on Windows; without
        // it every subprocess must be told the sandbox is off or it refuses to start.
        no_sandbox: 1,
        // tao owns the message loop on this thread, so CEF gets its own UI thread.
        multi_threaded_message_loop: 1,
        // One folder for everything, the same shape the WebView2 build used, so the
        // footprint is one directory and a profile comparison is like for like.
        root_cache_path: CefString::from(&*data_dir.join("cef").to_string_lossy()),
        cache_path: CefString::from(&*data_dir.join("cef").to_string_lossy()),
        persist_session_cookies: 1,
        locale: CefString::from("en-US"),
        // WARNING and a file, still: this log is what made the 2026-09-09 fault
        // diagnosable at all, and CEF writes to `debug.log` beside the executable if it
        // is given no path, which is worse than writing to one we watch. What changed is
        // that something now watches it - CEF itself neither rotates nor caps.
        log_severity: LogSeverity::WARNING,
        log_file: CefString::from(&*watchdog::log_path(&data_dir).to_string_lossy()),
        ..Default::default()
    };
    // The bundled runtime lives in the folder the exe unpacked itself into (or beside the
    // exe for a build straight out of target\release), never in a Chrome install.
    settings.resources_dir_path = CefString::from(&*cef_dir.to_string_lossy());
    settings.locales_dir_path = CefString::from(&*cef_dir.join("locales").to_string_lossy());

    // A debugging port is how the frame-timing and memory probes get into the page.
    // Off unless asked for: it is an open localhost socket into the logged-in session.
    if let Ok(port) = std::env::var("WHATSAPP_RS_DEBUG_PORT") {
        if let Ok(p) = port.parse::<i32>() {
            settings.remote_debugging_port = p;
        }
    }

    // Park the window for the CEF UI thread; see the comment on PARENT_HWND for why the
    // browser cannot simply be created here.
    let size = window.inner_size();
    PARENT_HWND.store(window.hwnd(), Ordering::SeqCst);
    INIT_W.store(size.width as i32, Ordering::SeqCst);
    INIT_H.store(size.height as i32, Ordering::SeqCst);

    let mut app = HostApp::new(RefCell::new(None));
    if initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
    ) != 1
    {
        return Err("cef initialize failed; see cef.log beside the profile".into());
    }

    // The engine started, so the folders older versions unpacked themselves into are dead
    // weight - ~350 MB each. Deleting them walks a lot of files, so it happens off the
    // startup path; nothing waits on it and a failure is not worth reporting.
    if !engine.beside_exe {
        let keep = engine.dir.clone();
        std::thread::spawn(move || crate::engine::sweep_old(&keep));
    }

    let tray = tray::build(tray::State {
        mute_sounds: current.mute_sounds,
        notifications: current.notifications,
        autostart: shortcut::autostart_enabled(),
    });
    {
        let proxy = event_loop.create_proxy();
        tray_icon::TrayIconEvent::set_event_handler(Some(move |event| {
            if let tray_icon::TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            } = event
            {
                let _ = proxy.send_event(UserEvent::TrayClick);
            }
        }));
    }
    if tray.is_some() {
        let proxy = event_loop.create_proxy();
        muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
            let ev = match event.id.as_ref() {
                tray::OPEN_ID => UserEvent::MenuOpen,
                tray::RELOAD_ID => UserEvent::MenuReload,
                tray::MUTE_ID => UserEvent::MenuMute,
                tray::NOTIFY_ID => UserEvent::MenuNotify,
                tray::AUTOSTART_ID => UserEvent::MenuAutostart,
                tray::ABOUT_ID => UserEvent::MenuAbout,
                tray::QUIT_ID => UserEvent::MenuQuit,
                _ => return,
            };
            let _ = proxy.send_event(ev);
        }));
    }

    // A measurement run asks for its own shutdown rather than being killed, because a
    // killed Chromium leaves its profile mid-write and can cost the login.
    let quit_after = std::env::var("WHATSAPP_RS_QUIT_AFTER")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(|s| Instant::now() + Duration::from_secs(s));

    let mut next_tick = Instant::now() + TICK;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(next_tick);

        match event {
            Event::NewEvents(StartCause::Init) => {}

            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                record_geometry(&window, &mut store);
                window.set_visible(false);
            }

            // The browser is a plain child window, so it does not follow our size on
            // its own. This is the one piece of plumbing owning the HWND costs us.
            Event::WindowEvent {
                event: WindowEvent::Resized(new_size),
                ..
            } => {
                resize_browser(new_size.width as i32, new_size.height as i32);
            }

            Event::UserEvent(UserEvent::TrayClick) => toggle(&window, &mut store),
            Event::UserEvent(UserEvent::MenuOpen) => show(&window),
            Event::UserEvent(UserEvent::MenuReload) => post_ui(UiJob::Reload),
            Event::UserEvent(UserEvent::MenuMute) => {
                current.mute_sounds = !current.mute_sounds;
                MUTED.store(current.mute_sounds, Ordering::SeqCst);
                prefs.save(current);
                if let Some(t) = &tray {
                    t.set_mute(current.mute_sounds);
                }
                post_ui(UiJob::Mute(current.mute_sounds));
            }
            Event::UserEvent(UserEvent::MenuNotify) => {
                current.notifications = !current.notifications;
                NOTIFICATIONS.store(current.notifications, Ordering::SeqCst);
                prefs.save(current);
                if let Some(t) = &tray {
                    t.set_notifications(current.notifications);
                }
            }
            Event::UserEvent(UserEvent::MenuAutostart) => {
                let want = !shortcut::autostart_enabled();
                if let Err(e) = shortcut::set_autostart(want, APP_ID) {
                    eprintln!("[whatsapp-rs] autostart: {e}");
                }
                // The Startup file is the truth; the check mark follows it, not the click.
                if let Some(t) = &tray {
                    t.set_autostart(shortcut::autostart_enabled());
                }
            }
            Event::UserEvent(UserEvent::MenuAbout) => open_url(REPO_URL),
            Event::UserEvent(UserEvent::MenuQuit) => {
                record_geometry(&window, &mut store);
                *control_flow = ControlFlow::Exit;
            }

            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                next_tick = Instant::now() + TICK;
                // A stat and a memory counter. The one tick's work that can never be
                // skipped: it is the only thing standing between a fault loop and the
                // user's system drive.
                watchdog.tick();
                if window.is_visible() {
                    record_geometry(&window, &mut store);
                }
                // poll_requests, NOT poll_show_request: the latter discards a queued quit,
                // so `whatsapp.exe --quit` would be silently ignored and every restart
                // would end in a kill - which skips Chromium's cookie flush and is how a
                // WhatsApp login gets lost.
                let requests = single_instance::poll_requests(&listener);
                if requests.show {
                    show(&window);
                }
                if requests.quit {
                    record_geometry(&window, &mut store);
                    *control_flow = ControlFlow::Exit;
                }
                if quit_after.map(|d| Instant::now() >= d).unwrap_or(false) {
                    record_geometry(&window, &mut store);
                    *control_flow = ControlFlow::Exit;
                }
            }

            Event::LoopDestroyed => {
                // Chromium flushes cookies and IndexedDB here. Skipping it is how a
                // WhatsApp login gets lost.
                shutdown();
            }

            _ => {}
        }
    });
}

/// Alloy, and the reason is measured, not preferred.
///
/// CEF 152 has two runtime styles. Chrome style is the one that carries Chromium's own
/// notification and permission machinery, so it was the obvious choice for a messaging
/// app. **It crashes.** With a browser created as a child of a native window
/// (`set_as_child`), Chrome style dies with an access violation (0xC0000005) immediately
/// after `create_browser` returns 1, twice out of two, with nothing in `cef.log`. The
/// identical build under Alloy style runs. Chrome style wants CEF's own Views window,
/// which is not a window we own, which is the whole point of this design.
///
/// What Alloy costs: its default answer to a permission prompt is IGNORE, so notifications
/// are silently dead unless the host answers them. That is what `HostPermissions` and the
/// `SetContentSetting` call in `on_context_initialized` are for.
///
/// `WHATSAPP_RS_CEF_STYLE=chrome` re-tests the crash on a future CEF without a rebuild.
fn runtime_style() -> RuntimeStyle {
    match std::env::var("WHATSAPP_RS_CEF_STYLE").as_deref() {
        Ok("chrome") => RuntimeStyle::CHROME,
        Ok("default") => RuntimeStyle::DEFAULT,
        _ => RuntimeStyle::ALLOY,
    }
}

/// At most this many toasts in any [`TOAST_WINDOW`], however many the page asks for.
///
/// Thirty a minute is already more than anyone reads, and a genuine loop - a broken page
/// script or a service worker replaying pushes - runs thousands a second, so the margin
/// between "busy group chat" and "runaway" is about four orders of magnitude. This is the
/// same defect the GPU switch had, on the notification pipe: an unbounded path driven by
/// something we do not control. Found while auditing for others like it, 2026-09-09.
const TOAST_BURST: u32 = 30;
const TOAST_WINDOW: Duration = Duration::from_secs(60);

/// Guards the burst counter. `raise_toast` runs on CEF's UI thread, not ours.
static TOAST_GATE: Mutex<ToastGate> = Mutex::new(ToastGate {
    window_start: None,
    raised: 0,
    dropped: 0,
    reported: false,
});

struct ToastGate {
    window_start: Option<Instant>,
    raised: u32,
    dropped: u32,
    /// Whether the user has been told, once, that notifications are being held back. The
    /// `eprintln!` below goes nowhere in a release build - `windows_subsystem = "windows"`
    /// means there is no console - so without this, suppressing somebody's messages would be
    /// completely invisible to them. Silent throttling of a messaging app is its own defect.
    reported: bool,
}

enum ToastVerdict {
    Allow,
    Drop,
    /// Drop, and raise one plain-language notice in its place. Happens once per run.
    DropAndSaySo,
}

fn toast_verdict() -> ToastVerdict {
    let Ok(mut gate) = TOAST_GATE.lock() else {
        // A poisoned lock must not silence a messaging app.
        return ToastVerdict::Allow;
    };
    let now = Instant::now();
    match gate.window_start {
        Some(start) if now.duration_since(start) < TOAST_WINDOW => {}
        _ => {
            if gate.dropped > 0 {
                eprintln!(
                    "[whatsapp-rs] dropped {} toasts over the burst limit",
                    gate.dropped
                );
            }
            gate.window_start = Some(now);
            gate.raised = 0;
            gate.dropped = 0;
        }
    }
    if gate.raised < TOAST_BURST {
        gate.raised += 1;
        return ToastVerdict::Allow;
    }
    gate.dropped += 1;
    if gate.reported {
        ToastVerdict::Drop
    } else {
        gate.reported = true;
        ToastVerdict::DropAndSaySo
    }
}

/// Draw the toast the page asked for.
///
/// Deliberately a hand-rolled two-field parse rather than pulling serde in for it: the
/// producer is `NOTIFY_SHIM_JS`, three lines above, and it emits exactly these two keys.
fn raise_toast(payload: &str) {
    fn field<'a>(payload: &'a str, key: &str) -> Option<&'a str> {
        let needle = format!("\"{key}\":\"");
        let start = payload.find(&needle)? + needle.len();
        let rest = &payload[start..];
        let mut end = 0;
        let bytes = rest.as_bytes();
        while end < bytes.len() {
            if bytes[end] == b'"' && (end == 0 || bytes[end - 1] != b'\\') {
                return Some(&rest[..end]);
            }
            end += 1;
        }
        None
    }
    let title = field(payload, "title").unwrap_or("WhatsApp");
    let body = field(payload, "body").unwrap_or_default();
    eprintln!("[whatsapp-rs] toast {title:?} / {body:?}");
    // The lock is released before anything is drawn: `notify::toast` is a blocking call into
    // Windows, and it has no business happening under a mutex that every notification takes.
    match toast_verdict() {
        ToastVerdict::Allow => {}
        ToastVerdict::Drop => return,
        ToastVerdict::DropAndSaySo => {
            notify::toast(
                "WhatsApp Rs",
                "WhatsApp Web is asking for more notifications than anyone could read, so they \
                 are being held back for now. Reload from the tray menu if that looks wrong.",
            );
            return;
        }
    }
    notify::toast(if title.is_empty() { "WhatsApp" } else { title }, body);
}

fn resize_browser(w: i32, h: i32) {
    let hwnd = BROWSER_HWND.load(Ordering::Relaxed);
    if hwnd == 0 {
        return;
    }
    unsafe {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOZORDER};
        let _ = SetWindowPos(HWND(hwnd as _), None, 0, 0, w, h, SWP_NOZORDER);
    }
}

/// Work for CEF's UI thread, posted from ours. Everything that touches the `Browser` goes
/// through here; calling it from the tao thread is the access violation described at
/// `PARENT_HWND`.
#[derive(Clone)]
enum UiJob {
    Reload,
    Mute(bool),
}

fn post_ui(job: UiJob) {
    let mut task = UiTask::new(job);
    if post_task(ThreadId::UI, Some(&mut task)) != 1 {
        eprintln!("[whatsapp-rs] post_task to the UI thread failed");
    }
}

wrap_task! {
    struct UiTask {
        job: UiJob,
    }

    impl Task {
        fn execute(&self) {
            let Ok(slot) = BROWSER.lock() else { return };
            let Some(UiBrowser(browser)) = slot.as_ref() else { return };
            match self.job {
                UiJob::Reload => browser.reload(),
                UiJob::Mute(on) => push_mute(browser, on),
            }
        }
    }
}

/// Tell the page whether to keep its chime down. UI thread only.
///
/// This is page-side on purpose. `BrowserHost::set_audio_muted` would do it in one call
/// and would also silence a voice call, which is the opposite of what "mute
/// notifications" means. The shim (`NOTIFY_SHIM_JS`) mutes only the short alert sounds.
fn push_mute(browser: &Browser, on: bool) {
    let Some(frame) = browser.main_frame() else {
        return;
    };
    let js = format!("if (window.__waRsSetMute) window.__waRsSetMute({on});");
    frame.execute_java_script(
        Some(&CefString::from(js.as_str())),
        Some(&CefString::from("whatsapp-rs://mute")),
        0,
    );
}

/// Hand a URL to the default browser. ShellExecute, not `cmd /c start`: the latter
/// flashes a console window, which this app never shows.
fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::core::{w, PCWSTR};
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let url_w: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(url_w.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

fn show(window: &Window) {
    window.set_visible(true);
    if window.is_minimized() {
        window.set_minimized(false);
    }
    window.set_focus();
}

fn toggle(window: &Window, store: &mut geometry::Store) {
    if window.is_visible() {
        record_geometry(window, store);
        window.set_visible(false);
    } else {
        show(window);
    }
}

fn record_geometry(window: &Window, store: &mut geometry::Store) {
    let scale = window.scale_factor();
    let size = window.inner_size().to_logical::<f64>(scale);
    let pos = match window.outer_position() {
        Ok(p) => p.to_logical::<f64>(scale),
        Err(_) => return,
    };
    if window.is_maximized() {
        return;
    }
    store.save_if_changed(geometry::Placement {
        x: pos.x as i32,
        y: pos.y as i32,
        w: size.width as u32,
        h: size.height as u32,
        maximized: false,
    });
}

#[derive(Default)]
struct HostState {
    browsers: usize,
}

/// Switches appended to the BROWSER process command line, and the reason each one is here.
///
/// Hoisted out of `on_before_command_line_processing` so a test can read it, and that is not
/// tidiness. `in-process-gpu` sat in this list for two days and cost a 244 GB log file, 10.9 GB
/// of RAM and 8.3 CPU-hours (DECISIONS.md #24), and nothing in this repository could have
/// noticed it was there. `in_process_gpu_is_never_shipped` below now fails in milliseconds if
/// it or `single-process` comes back.
const BROWSER_SWITCHES: &[&str] = &[
    "disable-background-networking",
    "disable-breakpad",
    "disable-component-update",
    "disable-domain-reliability",
    "disable-sync",
    "no-pings",
    "no-default-browser-check",
    "no-first-run",
    // The process trim, measured 2026-09-07 across five configurations:
    // 7 processes and 550 MB stock, 5 processes and 501 MB with these two plus
    // `in-process-gpu`.
    //
    // ⛔ `in-process-gpu` WAS HERE AND IS GONE. It cost 32 MB and one process,
    // and on 2026-09-09 it cost a 244 GB log file, 10.9 GB of RAM and 8.3
    // CPU-hours in a single unbounded loop. The switch moves Chromium's GPU
    // service onto a thread of this process, and
    // `viz::GpuServiceImpl::MaybeExitOnContextLost` opens with, in effect,
    // "if we are in the host process, we cannot restart the GPU process, so
    // just hope for recovery" - and returns. That early return IS Chromium's
    // circuit breaker being switched off. Out of process, the same context
    // loss makes the GPU process exit, `GpuProcessHost::RecordProcessCrash`
    // counts it, and after about three crashes Chromium falls back to
    // SwiftShader for the rest of the session. In process there is no process
    // to exit, no counter, and no fallback: when an NVIDIA driver reset took
    // the D3D11 device away at 04:10:37, `eglCreateContext` was retried about
    // seven thousand times a second for the next eight hours and forty-five
    // minutes. Full evidence in the incident report; the backstop that bounds
    // the damage if this ever happens again is `watchdog.rs`.
    //
    // `WHATSAPP_RS_CEF_SWITCHES=in-process-gpu` still reaches it, for a
    // measurement run only. It must not come back.
    //
    // Still NOT here: `--disable-gpu`. It saved slightly more and drops
    // Chromium onto a software rasteriser, which is the shape of the mistake
    // the Servo round made - a memory "win" that cost the frame rate. Software
    // rendering is where Chromium ends up on its own after three real GPU
    // failures, which is the right time to arrive there and not before.
    //
    // Deliberately NOT here: `--single-process`, which measures 352 MB in one
    // process. The owner ruled it out (2026-09-07) and he is right to: it is
    // unsupported by Chromium and a renderer crash takes the whole app down
    // instead of showing an error page. Note that it implies `in-process-gpu`
    // and so carries the same defect.
    //
    // Also not here: `--enable-low-end-device-mode`. It was 4 MB better than
    // this set and doubled the time to first paint, and it shrinks tile and
    // cache budgets in ways that would show up on a long chat list rather than
    // on the login page this was measured against.
    "process-per-site",
];

/// Switches that put Chromium's GPU service on a thread of this process, which deletes its
/// own context-loss circuit breaker. Neither may ever appear in [`BROWSER_SWITCHES`]; both
/// stay reachable through `WHATSAPP_RS_CEF_SWITCHES` for a measurement run, loudly.
const GPU_IN_HOST_SWITCHES: &[&str] = &["in-process-gpu", "single-process"];

wrap_app! {
    pub struct HostApp {
        client: RefCell<Option<Client>>,
    }

    impl App {
        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(HostProcessHandler::new(self.client.clone()))
        }

        /// Lives in the RENDER process, which is the only place that can touch the page's
        /// JavaScript before the page's own scripts run.
        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(HostRenderHandler::new())
        }

        /// Switches applied to every process, including the ones CEF spawns. The host
        /// process cannot add them any other way: CEF builds its children's command
        /// lines itself.
        fn on_before_command_line_processing(
            &self,
            process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            let Some(cmd) = command_line else { return };
            let is_browser = process_type.map(|p| p.to_string().is_empty()).unwrap_or(true);
            if !is_browser {
                return;
            }
            // A single-site viewer needs none of Chrome's own services. Every one of
            // these is measured in the sweep rather than assumed to help; the list and the
            // reasoning are at BROWSER_SWITCHES.
            for switch in BROWSER_SWITCHES {
                cmd.append_switch(Some(&CefString::from(*switch)));
            }
            cmd.append_switch_with_value(
                Some(&CefString::from("renderer-process-limit")),
                Some(&CefString::from("1")),
            );
            cmd.append_switch_with_value(
                Some(&CefString::from("disable-features")),
                Some(&CefString::from(concat!(
                    "Translate,MediaRouter,OptimizationHints,",
                    "OptimizationGuideModelDownloading,InterestFeedContentSuggestions,",
                    "CalculateNativeWinOcclusion,AutofillServerCommunication",
                ))),
            );
            // Extra switches for a measurement run, so a sweep needs no rebuild.
            if let Ok(extra) = std::env::var("WHATSAPP_RS_CEF_SWITCHES") {
                for part in extra.split(',').filter(|s| !s.trim().is_empty()) {
                    // `trim_start_matches('-')`: typing `--in-process-gpu` is the natural
                    // habit from a Chromium command line, and without this the guard below
                    // misses it - a silent bypass of the mechanism built to stop exactly
                    // that. `append_switch` adds the dashes itself, so both spellings work.
                    let key = part
                        .split_once('=')
                        .map(|(k, _)| k)
                        .unwrap_or(part)
                        .trim()
                        .trim_start_matches('-');
                    // Deliberate is fine. SILENT is not: a stale environment variable or a
                    // shortcut somebody made during a sweep would otherwise re-arm the
                    // 2026-09-09 defect with nothing anywhere saying so.
                    if GPU_IN_HOST_SWITCHES.contains(&key) {
                        eprintln!(
                            "[whatsapp-rs] ⛔ WHATSAPP_RS_CEF_SWITCHES is re-enabling --{key}. \
                             That puts Chromium's GPU on a thread of this process and removes \
                             its context-loss circuit breaker; a GPU driver reset then runs \
                             away unbounded. Measurement runs only. See DECISIONS.md #24."
                        );
                    }
                    match part.split_once('=') {
                        Some((k, v)) => cmd.append_switch_with_value(
                            Some(&CefString::from(k.trim())),
                            Some(&CefString::from(v.trim())),
                        ),
                        None => cmd.append_switch(Some(&CefString::from(part.trim()))),
                    }
                }
            }
        }
    }
}

/// Tag the shim prints and the host watches for. Long and unmistakable on purpose: it is
/// matched against every console line the page produces.
const NOTIFY_TAG: &str = "WHATSAPP_RS_NOTIFY ";

/// Turn the page's notifications into something the host can raise as a Windows toast.
///
/// CEF has no notification callback. `CefClient` declares eighteen `Get*Handler` factories
/// and none of them is about notifications; `CefPermissionHandler` covers the *permission*
/// and nothing else. So there is no supported way to be handed a notification's title and
/// body - which is exactly what the host needs, because a Windows toast only appears if
/// somebody raises it under a registered AppUserModelID (see notify.rs and shortcut.rs).
///
/// The channel used here is the page's own console, which already reaches the host through
/// `DisplayHandler::on_console_message`. It is one-way and it is a hack, and it is chosen
/// over CEF's message router because it needs no V8 native binding, no second IPC path and
/// no extra process wiring - the console line is already crossing that boundary.
///
/// The real `Notification` is still constructed, so `onshow`/`onclick` behave for the page.
/// It has to POLYFILL, not merely wrap. CEF's Alloy runtime calls
/// `WebRuntimeFeatures::EnableNotifications(false)` in its renderer client, so in an Alloy
/// browser `window.Notification` is not a thing that works badly - it is a thing that may
/// not exist. A shim that starts with `if (typeof Notification === 'undefined') return;`
/// therefore does nothing at all, and WhatsApp Web, finding no Notification API, turns its
/// alerts off. So: wrap the real one if there is one, and install a complete stand-in if
/// there is not.
const NOTIFY_SHIM_JS: &str = r#"
(function () {
  if (window.__waRsNotifyBridged) return;
  window.__waRsBridged = true;
  window.__waRsNotifyBridged = true;

  function send(title, options) {
    options = options || {};
    try {
      console.log('WHATSAPP_RS_NOTIFY ' + JSON.stringify({
        title: String(title == null ? '' : title),
        body: String(options.body == null ? '' : options.body),
        tag: String(options.tag == null ? '' : options.tag)
      }));
    } catch (e) {}
  }

  var Real = (typeof Notification !== 'undefined') ? Notification : null;

  function Shim(title, options) {
    if (!(this instanceof Shim)) return new Shim(title, options);
    options = options || {};
    this.title = title; this.body = options.body; this.tag = options.tag;
    this.onshow = this.onclick = this.onclose = this.onerror = null;
    send(title, options);
    if (Real) { try { this._real = new Real(title, options); } catch (e) {} }
    var self = this;
    // The page expects an async onshow. Firing it synchronously from the constructor
    // would run before the caller has assigned its handler.
    setTimeout(function () { if (typeof self.onshow === 'function') { try { self.onshow(); } catch (e) {} } }, 0);
  }
  Shim.prototype.close = function () { if (this._real) { try { this._real.close(); } catch (e) {} } };
  Shim.prototype.addEventListener = function (name, fn) { if (name === 'show') this.onshow = fn; };
  Shim.prototype.removeEventListener = function () {};

  // The host raises the toast, so as far as the page is concerned permission is granted.
  // Claiming "granted" while nothing is displayed would be the WebView2 lie all over
  // again; here it is true because raise_toast() in cef_view.rs really does draw one.
  Shim.permission = 'granted';
  Shim.requestPermission = function (cb) {
    var p = Promise.resolve('granted');
    if (typeof cb === 'function') p.then(cb);
    return p;
  };
  Shim.maxActions = 2;

  // Plain assignment is not enough and that was measured, not assumed: with
  // `window.Notification = Shim` the shim demonstrably ran (its marker was on the page)
  // while `Notification` was still the engine's own constructor afterwards. defineProperty
  // states the intent exactly, and the result is reported on the console so a future
  // failure is visible instead of silent.
  function install() {
    try {
      Object.defineProperty(window, 'Notification', {
        value: Shim, writable: true, configurable: true, enumerable: false
      });
    } catch (e) {
      try { window.Notification = Shim; } catch (e2) {}
    }
    // WhatsApp also references the service-worker path. A worker global cannot be reached
    // from here, but the registration object the PAGE holds can, and that is the one the
    // page's own code calls showNotification on. Patched inside install() so it is
    // re-applied alongside the constructor rather than only once.
    try {
      var proto = window.ServiceWorkerRegistration && ServiceWorkerRegistration.prototype;
      if (proto && !proto.__waRsPatched) {
        var realShow = proto.showNotification;
        proto.showNotification = function (title, options) {
          send(title, options);
          if (realShow) { try { return realShow.call(this, title, options); } catch (e) {} }
          return Promise.resolve();
        };
        if (!proto.getNotifications) {
          proto.getNotifications = function () { return Promise.resolve([]); };
        }
        proto.__waRsPatched = true;
      }
    } catch (e) {}
    try {
      return !/\[native code\]/.test(Function.prototype.toString.call(window.Notification));
    } catch (e) { return false; }
  }
  var ok = install();
  // A page that replaces the constructor after us would silently undo it, so check again
  // once the document is up rather than trusting the first result forever.
  try {
    document.addEventListener('DOMContentLoaded', function () {
      if (!/\[native code\]/.test(Function.prototype.toString.call(window.Notification))) return;
      console.log('WHATSAPP_RS_SHIM reinstalling, something replaced Notification');
      install();
    });
  } catch (e) {}
  // ---- "Mute sounds", the tray toggle. ----
  // Read from the bundles the page actually loaded (tools/script-grep.py, 2026-09-08):
  // every alert is a module-level `new window.Audio(<static asset URL>)` - the message
  // chime (WAWebNotificationTone), the sent tone (WAWebOutgoingMessageTone), the voice-
  // note start/end beeps (WAWebPttPlaybackTone), the call-end tone (WAWebCallEndTone) and
  // the ringtone. Voice messages are end-to-end encrypted and so are always decrypted in
  // the page and played from blob: URLs, and a call's remote audio arrives as a
  // MediaStream through srcObject. "A detached media element playing a plain https
  // source" is therefore exactly the set of tones and nothing else, which is why muting
  // is done here and not with the engine's whole-page audio mute.
  window.__waRsMuted = !!window.__waRsMuted;
  window.__waRsSetMute = function (on) { window.__waRsMuted = !!on; };
  try {
    var mproto = window.HTMLMediaElement && HTMLMediaElement.prototype;
    if (mproto && !mproto.__waRsMutePatched) {
      var realPlay = mproto.play;
      mproto.play = function () {
        try {
          var src = this.currentSrc || this.src || '';
          var tone = !this.isConnected && !this.srcObject && /^https?:/.test(src);
          if (tone) {
            if (window.__waRsMuted) { this.muted = true; this.__waRsForced = true; }
            else if (this.__waRsForced) { this.muted = false; this.__waRsForced = false; }
          }
        } catch (e) {}
        return realPlay.apply(this, arguments);
      };
      mproto.__waRsMutePatched = true;
    }
  } catch (e) {}

  try { console.log('WHATSAPP_RS_SHIM installed=' + ok); } catch (e) {}

})();
"#;

wrap_render_process_handler! {
    struct HostRenderHandler;

    impl RenderProcessHandler {
        /// Document start, in the render process. Injecting from the browser process on
        /// `OnLoadStart` would be a race against the page's own scripts.
        fn on_context_created(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _context: Option<&mut V8Context>,
        ) {
            if std::env::var("WHATSAPP_RS_NOTIFY_BRIDGE").as_deref() == Ok("off") {
                return;
            }
            let Some(frame) = frame else { return };
            frame.execute_java_script(
                Some(&CefString::from(NOTIFY_SHIM_JS)),
                Some(&CefString::from("whatsapp-rs://notify-shim")),
                0,
            );
        }
    }
}

wrap_browser_process_handler! {
    struct HostProcessHandler {
        client: RefCell<Option<Client>>,
    }

    impl BrowserProcessHandler {
        /// Runs on CEF's UI thread once the context exists. Everything that touches a
        /// request context or creates a browser has to happen here, not on the thread
        /// that called `initialize`: doing it there crashed the process with an access
        /// violation and nothing in the log.
        fn on_context_initialized(&self) {
            debug_assert_ne!(currently_on(ThreadId::UI), 0);

            // Pre-grant notifications for WhatsApp only, so `Notification.permission`
            // reads "granted" from the first paint rather than "default" until the page
            // asks. The permission handler is the belt to this pair of braces.
            if let Some(ctx) = request_context_get_global_context() {
                ctx.set_content_setting(
                    Some(&CefString::from(notify::APP_URL)),
                    Some(&CefString::from(notify::APP_URL)),
                    ContentSettingTypes::NOTIFICATIONS,
                    ContentSettingValues::ALLOW,
                );
            }

            let parent = PARENT_HWND.load(Ordering::SeqCst);
            let window_info = WindowInfo {
                // Chrome style carries Chromium's real notification and permission
                // machinery; Alloy's default answer to a permission prompt is to ignore
                // it, which silently disables notifications in a messaging app.
                runtime_style: runtime_style(),
                ..Default::default()
            }
            .set_as_child(
                // cef_window_handle_t is a newtype around a raw pointer on Windows, and
                // tao hands the HWND back as an isize.
                sys::HWND(parent as *mut sys::HWND__),
                &Rect {
                    x: 0,
                    y: 0,
                    width: INIT_W.load(Ordering::SeqCst),
                    height: INIT_H.load(Ordering::SeqCst),
                },
            );

            let mut client = HostClient::new(Arc::new(Mutex::new(HostState::default())));
            *self.client.borrow_mut() = Some(client.clone());
            let browser_settings = BrowserSettings::default();
            let url = CefString::from(notify::APP_URL);
            let ok = browser_host_create_browser(
                Some(&window_info),
                Some(&mut client),
                Some(&url),
                Some(&browser_settings),
                None,
                None,
            );
            eprintln!("[whatsapp-rs] create_browser parent={parent:#x} -> {ok}");
        }
    }
}

wrap_client! {
    pub struct HostClient {
        inner: Arc<Mutex<HostState>>,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(HostLifeSpan::new(self.inner.clone()))
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(HostDisplay::new())
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(HostLoad::new())
        }

        /// Without this, a permission request in Alloy style is silently ignored and in
        /// Chrome style raises a prompt inside a window that has no browser UI to show
        /// it in. Either way the page never gets an answer.
        fn permission_handler(&self) -> Option<PermissionHandler> {
            Some(HostPermissions::new())
        }
    }
}

wrap_life_span_handler! {
    struct HostLifeSpan {
        inner: Arc<Mutex<HostState>>,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                if let Some(host) = browser.host() {
                    let hwnd = host.window_handle();
                    BROWSER_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
                }
                if let Ok(mut slot) = BROWSER.lock() {
                    *slot = Some(UiBrowser(browser.clone()));
                }
            }
            if let Ok(mut state) = self.inner.lock() {
                state.browsers += 1;
            }
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            if let Ok(mut state) = self.inner.lock() {
                state.browsers = state.browsers.saturating_sub(1);
            }
            if let Ok(mut slot) = BROWSER.lock() {
                *slot = None;
            }
            BROWSER_HWND.store(0, Ordering::Relaxed);
        }
    }
}

wrap_load_handler! {
    struct HostLoad;

    impl LoadHandler {
        /// Every load, including WhatsApp's own reloads, starts with a page that knows
        /// nothing about the mute toggle. Re-tell it once the main frame is up.
        fn on_load_end(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: ::std::os::raw::c_int,
        ) {
            let (Some(browser), Some(frame)) = (browser, frame) else {
                return;
            };
            if frame.is_main() == 0 {
                return;
            }
            push_mute(browser, MUTED.load(Ordering::SeqCst));
        }
    }
}

wrap_display_handler! {
    struct HostDisplay;

    impl DisplayHandler {
        /// A GUI app has no console, so without this every error the page reports goes
        /// nowhere. On the Servo build that turned a five-minute diagnosis into a day.
        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            level: LogSeverity,
            message: Option<&CefString>,
            source: Option<&CefString>,
            line: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let text = message.map(CefString::to_string).unwrap_or_default();
            let src = source.map(CefString::to_string).unwrap_or_default();
            // The notification bridge's return path. See NOTIFY_SHIM_JS for why the
            // console is the channel.
            if let Some(payload) = text.strip_prefix(NOTIFY_TAG) {
                // "Show notifications" off: the page still believes it notified, which
                // keeps its own unread logic intact; we simply draw nothing.
                if NOTIFICATIONS.load(Ordering::SeqCst) {
                    raise_toast(payload);
                }
                return 1;
            }

            eprintln!("[console {:?}] {text}  ({src}:{line})", level.get_raw());
            // 0 = also let CEF log it normally.
            0
        }
    }
}

wrap_permission_handler! {
    struct HostPermissions;

    impl PermissionHandler {
        fn on_show_permission_prompt(
            &self,
            _browser: Option<&mut Browser>,
            _prompt_id: u64,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> ::std::os::raw::c_int {
            let origin = requesting_origin.map(CefString::to_string).unwrap_or_default();
            let ours = origin.starts_with(notify::APP_URL);
            eprintln!("[whatsapp-rs] permission prompt origin={origin} bits={requested_permissions:#x} ours={ours}");
            let Some(callback) = callback else { return 0 };
            // Answer immediately and without UI. Only WhatsApp itself is granted; any
            // other origin is denied rather than left hanging.
            callback.cont(if ours {
                PermissionRequestResult::ACCEPT
            } else {
                PermissionRequestResult::DENY
            });
            1
        }

        /// Camera and microphone, which is what a voice call asks for. Granting it here
        /// is the difference between a call starting and a silent failure.
        fn on_request_media_access_permission(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut MediaAccessCallback>,
        ) -> ::std::os::raw::c_int {
            let origin = requesting_origin.map(CefString::to_string).unwrap_or_default();
            let ours = origin.starts_with(notify::APP_URL);
            eprintln!("[whatsapp-rs] media access origin={origin} bits={requested_permissions:#x} ours={ours}");
            let Some(callback) = callback else { return 0 };
            if ours {
                callback.cont(requested_permissions);
            } else {
                callback.cancel();
            }
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BROWSER_SWITCHES, GPU_IN_HOST_SWITCHES};

    /// The check this repository did not have on 2026-09-07, and the whole reason
    /// `BROWSER_SWITCHES` is a named constant instead of an array literal inside a closure.
    ///
    /// `in-process-gpu` shipped for two days. It moves Chromium's GPU service onto a thread
    /// of the browser process, and `viz::GpuServiceImpl::MaybeExitOnContextLost` returns
    /// early when the service is in the host process - so a fatal context loss is never
    /// counted, the GPU is never restarted, and the fallback to software rendering never
    /// happens. A driver reset then retried `eglCreateContext` about seven thousand times a
    /// second for eight and three quarter hours: 244 GB of log, 10.9 GB of RAM, 8.3
    /// CPU-hours. `single-process` implies the same thing and is banned with it.
    #[test]
    fn in_process_gpu_is_never_shipped() {
        for banned in GPU_IN_HOST_SWITCHES {
            assert!(
                !BROWSER_SWITCHES.contains(banned),
                "--{banned} is back in BROWSER_SWITCHES. It deletes Chromium's context-loss \
                 circuit breaker; see DECISIONS.md #24 and the incident report."
            );
        }
    }

    /// `append_switch` adds the dashes itself, so a leading `--` here becomes `----gpu` and
    /// Chromium silently ignores it - a switch that reads as present and does nothing.
    #[test]
    fn browser_switches_carry_no_leading_dashes() {
        for switch in BROWSER_SWITCHES {
            assert!(
                !switch.starts_with('-'),
                "{switch:?} carries its own dashes; append_switch adds them"
            );
        }
    }
}
