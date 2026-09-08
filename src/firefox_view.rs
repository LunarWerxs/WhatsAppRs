//! Safe mode on a bundled Firefox: a real Firefox we ship, adopted into our window.
//!
//! The second candidate from DECISIONS.md #15, and the asymmetric one. Gecko has had no
//! supported embedding API since Mozilla ended the embedding project, and GeckoView is
//! Android only, so there is no way to put Firefox inside our process the way CEF goes
//! inside it. A "Firefox version" can only mean: ship a real Firefox, run it as a
//! separate process, and take its window.
//!
//! What that costs, stated plainly, because it is the thing that decides this:
//!
//! - **We find another process's window and adopt it.** `SetParent` across processes is
//!   supported on Windows and, measured here, holds: the Firefox window sits inside our
//!   window, resizes with it, and keeps rendering. It is still a hack, and it is the
//!   `WindowFinder` class the C# original had and the current design deleted.
//! - **Two processes must die together.** A job object with kill-on-close does that, so
//!   a crashed or killed host cannot leave an orphan browser holding the profile.
//! - **The browser chrome is removed with CSS, not an API.** Firefox has no site-specific
//!   browser mode any more (removed in 86) and `-kiosk` is fullscreen-only, so the tab
//!   bar and URL bar are hidden by a `userChrome.css` in the profile we write.
//!
//! Everything else the app owns - tray, close-to-tray, single instance, geometry - works
//! unchanged, because the window the user interacts with is still ours.

use crate::{geometry, notify, paths, shortcut, single_instance, tray, APP_ID};

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use tao::dpi::{LogicalPosition, LogicalSize};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::windows::WindowExtWindows;
use tao::window::{Window, WindowBuilder};

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
    JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowLongW, GetWindowThreadProcessId, IsWindow,
    IsWindowVisible, PostMessageW, SetParent, SetWindowLongW, SetWindowPos, GWL_STYLE,
    // SetFocus lives in KeyboardAndMouse, not WindowsAndMessaging; imported below.
    SWP_NOZORDER, SWP_SHOWWINDOW, WM_CLOSE, WS_CAPTION, WS_CHILD, WS_POPUP, WS_THICKFRAME,
};

const DEFAULT_W: f64 = 1180.0;
const DEFAULT_H: f64 = 860.0;
const TICK: Duration = Duration::from_millis(1500);
/// Firefox's top-level window class. Confirmed by reading it off a running instance
/// rather than taken from documentation.
const FIREFOX_WINDOW_CLASS: &str = "MozillaWindowClass";

#[derive(Debug)]
enum UserEvent {
    TrayClick,
    MenuOpen,
    MenuQuit,
}

pub(crate) fn run() -> Result<(), String> {
    let listener = match single_instance::acquire() {
        single_instance::Instance::Second => return Ok(()),
        single_instance::Instance::First(l) => l,
    };

    notify::set_app_user_model_id(APP_ID);
    let _ = shortcut::ensure(APP_ID);

    let firefox = firefox_exe()?;
    let data_dir = paths::data_dir();
    let profile = data_dir.join("firefox-profile");
    write_profile(&profile)?;

    let mut store = geometry::Store::new(&data_dir);
    let saved = store.load();

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();

    let mut window_builder = WindowBuilder::new()
        .with_title("WhatsApp")
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

    // Everything Firefox spawns joins this job, so closing the host - cleanly, by crash,
    // or by Task Manager - takes the whole browser with it. Without this, a killed host
    // leaves a Firefox holding the profile lock and the next launch cannot start.
    let job = Job::new()?;

    let mut command = Command::new(&firefox);
    command
        .arg("-profile")
        .arg(&profile)
        // -no-remote became a no-op in Firefox 131; -new-instance is the current flag.
        .arg("-new-instance")
        .arg(notify::APP_URL);
    if let Ok(port) = std::env::var("WHATSAPP_RS_DEBUG_PORT") {
        command.arg("--remote-debugging-port").arg(port);
    }
    let child = command
        .spawn()
        .map_err(|e| format!("could not start the bundled Firefox: {e}"))?;
    job.adopt(child.id())?;

    // Firefox re-execs itself through a launcher process, so the window belongs to a
    // different pid than the one we spawned. Match on the executable path instead, which
    // also keeps a Firefox the user installed themselves out of the search.
    eprintln!("[whatsapp-rs] waiting for a window from {}", firefox.display());
    let browser_hwnd = wait_for_window(&firefox, Duration::from_secs(60)).ok_or_else(|| {
        format!(
            "the bundled Firefox never opened a window we could find.

Looking for: {}
Mozilla windows seen: {:?}",
            firefox.display(),
            list_mozilla_windows()
        )
    })?;
    eprintln!("[whatsapp-rs] adopting firefox window {:?}", browser_hwnd.0);
    adopt(browser_hwnd, HWND(window.hwnd() as _));
    let size = window.inner_size();
    place(browser_hwnd, size.width as i32, size.height as i32);

    let tray = tray::build();
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
    if let Some(t) = &tray {
        let proxy = event_loop.create_proxy();
        let open_id = t.open_id.clone();
        let quit_id = t.quit_id.clone();
        muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
            if event.id == open_id {
                let _ = proxy.send_event(UserEvent::MenuOpen);
            } else if event.id == quit_id {
                let _ = proxy.send_event(UserEvent::MenuQuit);
            }
        }));
    }

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

            Event::WindowEvent {
                event: WindowEvent::Resized(new_size),
                ..
            } => {
                place(browser_hwnd, new_size.width as i32, new_size.height as i32);
            }

            // Activating OUR window does not give the adopted child keyboard focus, so
            // without this the app comes back from the tray and typing goes nowhere.
            // An embedded engine gets this for free; a foreign window does not.
            Event::WindowEvent {
                event: WindowEvent::Focused(true),
                ..
            } => unsafe {
                let _ = SetFocus(Some(browser_hwnd));
            },

            Event::UserEvent(UserEvent::TrayClick) => toggle(&window, &mut store),
            Event::UserEvent(UserEvent::MenuOpen) => show(&window),
            Event::UserEvent(UserEvent::MenuQuit) => {
                record_geometry(&window, &mut store);
                *control_flow = ControlFlow::Exit;
            }

            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                next_tick = Instant::now() + TICK;
                if window.is_visible() {
                    record_geometry(&window, &mut store);
                }
                // poll_requests, NOT poll_show_request: the latter discards a queued quit,
                // so `whatsapp.exe --quit` would be silently ignored and every restart
                // would end in a kill - which skips the profile flush and is how a
                // WhatsApp login gets lost.
                let requests = single_instance::poll_requests(&listener);
                if requests.show {
                    show(&window);
                }
                if requests.quit {
                    record_geometry(&window, &mut store);
                    *control_flow = ControlFlow::Exit;
                }
                // Firefox crashing, or the user quitting it some other way, must not
                // leave an empty frame sitting there pretending to be the app.
                if unsafe { !IsWindow(Some(browser_hwnd)).as_bool() } {
                    *control_flow = ControlFlow::Exit;
                }
                if quit_after.map(|d| Instant::now() >= d).unwrap_or(false) {
                    record_geometry(&window, &mut store);
                    *control_flow = ControlFlow::Exit;
                }
            }

            Event::LoopDestroyed => {
                // Ask first: Firefox flushes its profile on a clean shutdown, and the
                // job object below is the guarantee, not the polite path.
                unsafe {
                    let _ = PostMessageW(Some(browser_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                std::thread::sleep(Duration::from_millis(1200));
                // Nothing else needed: tao exits the process here, Windows closes our
                // last handle to the job, and kill-on-close takes every Firefox process
                // with it. `job` is owned by this closure purely to keep it open until now.
                let _ = &job;
            }

            _ => {}
        }
    });
}

/// The bundled engine: `firefox/firefox.exe` beside our executable.
fn firefox_exe() -> Result<PathBuf, String> {
    if let Ok(explicit) = std::env::var("WHATSAPP_RS_FIREFOX") {
        let p = PathBuf::from(explicit);
        if p.exists() {
            return Ok(p);
        }
    }
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "executable has no parent directory".to_string())?;
    for candidate in [
        dir.join("firefox").join("firefox.exe"),
        // Running out of target/<profile>/ during development, with the runtime kept
        // out of the build directory so a `cargo clean` does not delete 345 MB.
        dir.join("../../runtime/firefox/firefox.exe"),
        dir.join("../../../runtime/firefox/firefox.exe"),
    ] {
        if candidate.exists() {
            let resolved = candidate
                .canonicalize()
                .map_err(|e| format!("canonicalize firefox path: {e}"))?;
            return Ok(strip_verbatim(&resolved));
        }
    }
    Err(format!(
        "the bundled Firefox is missing: no firefox/firefox.exe beside {}",
        exe.display()
    ))
}

/// Write the private profile this app runs on.
///
/// It is ours, not the user's: a Firefox they installed is untouched, and this one holds
/// only the WhatsApp session. `user.js` is applied at every start, which is why the
/// settings live there rather than in `prefs.js`.
fn write_profile(profile: &Path) -> Result<(), String> {
    let chrome = profile.join("chrome");
    std::fs::create_dir_all(&chrome).map_err(|e| format!("profile dir: {e}"))?;

    let mut prefs = String::from(
        r#"// Written by WhatsApp at every launch. Edits here are overwritten.
user_pref("toolkit.legacyUserProfileCustomizations.stylesheets", true);
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("browser.startup.homepage_override.mstone", "ignore");
user_pref("browser.aboutwelcome.enabled", false);
user_pref("browser.sessionstore.resume_from_crash", false);
user_pref("datareporting.policy.dataSubmissionEnabled", false);
user_pref("datareporting.healthreport.uploadEnabled", false);
user_pref("app.shield.optoutstudies.enabled", false);
user_pref("app.update.auto", false);
user_pref("browser.discovery.enabled", false);
user_pref("extensions.pocket.enabled", false);
user_pref("toolkit.telemetry.enabled", false);
user_pref("toolkit.telemetry.unified", false);
user_pref("browser.newtabpage.activity-stream.feeds.telemetry", false);
user_pref("browser.contentblocking.category", "standard");
// Permissions must be pre-granted, and this is not a convenience.
//
// Firefox asks for the microphone, camera and notifications with a doorhanger anchored to
// the URL bar - which this profile's userChrome.css collapses, because a browser bar is
// exactly what an app must not have. Measured: the microphone request from a WebRTC probe
// simply never gets an answer, so a voice call would hang with no visible prompt.
//
// These are `default` grants, i.e. every site, not just WhatsApp. That is only acceptable
// because this profile is ours and never loads anything else; the app opens exactly one
// URL and has no way to navigate away.
user_pref("permissions.default.microphone", 1);
user_pref("permissions.default.camera", 1);
user_pref("permissions.default.desktop-notification", 1);
user_pref("media.navigator.permission.disabled", true);
"#,
    );

    // The trimming set is separate and switchable, because "this pref saves memory" is
    // exactly the kind of claim that measured worse than the default on the Servo build.
    if std::env::var("WHATSAPP_RS_FF_STOCK").is_err() {
        prefs.push_str(
            r#"// Trimming. One site, one tab, so the spare content processes are waste.
user_pref("dom.ipc.processCount", 1);
user_pref("dom.ipc.processPrelaunch.enabled", false);
user_pref("browser.tabs.unloadOnLowMemory", true);
user_pref("browser.sessionhistory.max_total_viewers", 0);
user_pref("browser.cache.memory.capacity", 32768);
"#,
        );
    }
    if let Ok(extra) = std::env::var("WHATSAPP_RS_FF_PREFS") {
        for line in extra.split(';').filter(|s| !s.trim().is_empty()) {
            if let Some((k, v)) = line.split_once('=') {
                prefs.push_str(&format!("user_pref(\"{}\", {});\n", k.trim(), v.trim()));
            }
        }
    }
    if std::env::var("WHATSAPP_RS_DEBUG_PORT").is_ok() {
        // 3 = CDP and WebDriver BiDi both. The default is BiDi only, and the frame
        // probe reuses the CDP client the WebView2 build already had.
        prefs.push_str("user_pref(\"remote.active-protocols\", 3);\n");
    }

    std::fs::write(profile.join("user.js"), prefs).map_err(|e| format!("user.js: {e}"))?;

    // No SSB mode exists any more and -kiosk is fullscreen-only, so the browser UI is
    // removed with CSS. visibility:collapse rather than display:none: the toolbars are
    // XUL boxes and display:none on them leaves layout artefacts.
    std::fs::write(
        chrome.join("userChrome.css"),
        r#"/* Written by WhatsApp. This is what makes a browser look like an app. */
#TabsToolbar, #nav-bar, #PersonalToolbar, #toolbar-menubar, #titlebar {
  visibility: collapse !important;
}
#navigator-toolbox { border: none !important; }
"#,
    )
    .map_err(|e| format!("userChrome.css: {e}"))?;

    Ok(())
}

/// Poll for a visible top-level Firefox window belonging to the executable we shipped.
fn wait_for_window(firefox: &Path, timeout: Duration) -> Option<HWND> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(hwnd) = find_window(firefox) {
            return Some(hwnd);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    None
}

struct Search {
    want: PathBuf,
    found: HWND,
}

fn find_window(firefox: &Path) -> Option<HWND> {
    let mut search = Search {
        want: firefox.to_path_buf(),
        found: HWND(std::ptr::null_mut()),
    };
    unsafe {
        let _ = EnumWindows(
            Some(enum_proc),
            LPARAM(&mut search as *mut Search as isize),
        );
    }
    if search.found.0.is_null() {
        None
    } else {
        Some(search.found)
    }
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
    let search = &mut *(lparam.0 as *mut Search);
    if !IsWindowVisible(hwnd).as_bool() {
        return true.into();
    }
    let mut class = [0u16; 128];
    let n = GetClassNameW(hwnd, &mut class);
    if n <= 0 || String::from_utf16_lossy(&class[..n as usize]) != FIREFOX_WINDOW_CLASS {
        return true.into();
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 {
        return true.into();
    }
    match process_image(pid) {
        Some(path) if same_file(&path, &search.want) => {
            search.found = hwnd;
            false.into()
        }
        _ => true.into(),
    }
}

/// Every Mozilla window on the desktop and which executable owns it. Only used to make a
/// failure say something, instead of "never opened a window".
fn list_mozilla_windows() -> Vec<String> {
    let mut seen = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(list_proc),
            LPARAM(&mut seen as *mut Vec<String> as isize),
        );
    }
    seen
}

unsafe extern "system" fn list_proc(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
    let seen = &mut *(lparam.0 as *mut Vec<String>);
    let mut class = [0u16; 128];
    let n = GetClassNameW(hwnd, &mut class);
    if n > 0 {
        let name = String::from_utf16_lossy(&class[..n as usize]);
        if name.starts_with("Mozilla") {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let image = process_image(pid)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<unknown>".into());
            seen.push(format!("{name} pid={pid} visible={} {image}", IsWindowVisible(hwnd).as_bool()));
        }
    }
    true.into()
}

fn process_image(pid: u32) -> Option<PathBuf> {
    unsafe {
        let handle: HANDLE = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        ok.ok()?;
        Some(PathBuf::from(String::from_utf16_lossy(
            &buf[..len as usize],
        )))
    }
}

/// Drop Windows' extended-length path prefix (backslash backslash question backslash).
///
/// `Path::canonicalize` always returns one, and `QueryFullProcessImageNameW` never does,
/// so comparing the two directly never matches. That cost a debugging round: the window
/// search silently found nothing, the launch failed, and the job object then killed the
/// Firefox that was on screen and working.
fn strip_verbatim(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path.to_path_buf(),
    }
}

/// Compare paths case-insensitively; Windows hands the image name back in whatever case
/// the filesystem stored, which is not the case we spawned it with.
fn same_file(a: &Path, b: &Path) -> bool {
    strip_verbatim(a).to_string_lossy().to_lowercase()
        == strip_verbatim(b).to_string_lossy().to_lowercase()
}

/// Turn a top-level window belonging to another process into a child of ours.
fn adopt(child: HWND, parent: HWND) {
    unsafe {
        let style = GetWindowLongW(child, GWL_STYLE);
        let stripped = style
            & !(WS_POPUP.0 as i32)
            & !(WS_CAPTION.0 as i32)
            & !(WS_THICKFRAME.0 as i32);
        SetWindowLongW(child, GWL_STYLE, stripped | WS_CHILD.0 as i32);
        let _ = SetParent(child, Some(parent));
    }
}

fn place(child: HWND, w: i32, h: i32) {
    unsafe {
        let _ = SetWindowPos(child, None, 0, 0, w, h, SWP_NOZORDER | SWP_SHOWWINDOW);
    }
}

/// A Windows job object that kills everything in it when the last handle closes.
struct Job(HANDLE);

impl Job {
    fn new() -> Result<Self, String> {
        unsafe {
            let handle =
                CreateJobObjectW(None, None).map_err(|e| format!("CreateJobObject: {e}"))?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .map_err(|e| format!("SetInformationJobObject: {e}"))?;
            Ok(Self(handle))
        }
    }

    fn adopt(&self, pid: u32) -> Result<(), String> {
        unsafe {
            let process = OpenProcess(
                windows::Win32::System::Threading::PROCESS_SET_QUOTA
                    | windows::Win32::System::Threading::PROCESS_TERMINATE,
                false,
                pid,
            )
            .map_err(|e| format!("OpenProcess: {e}"))?;
            let result = AssignProcessToJobObject(self.0, process);
            let _ = CloseHandle(process);
            result.map_err(|e| format!("AssignProcessToJobObject: {e}"))
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
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
