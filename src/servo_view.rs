//! Safe mode on Servo: WhatsApp's real website in a browser engine we build
//! ourselves, the same one on every operating system.
//!
//! DECISIONS.md #8: the engine is Servo, our own build of the `cache-storage-complete`
//! branch at `../servo`, with the four patches that make web.whatsapp.com load.
//! Nothing here belongs to Microsoft, Google or Apple, and the phone sees the
//! same device on Windows, macOS and Linux. The page is served because Servo
//! presents as Firefox, which is on WhatsApp's supported list; the OS token in
//! that string is the only per-platform difference and it is truthful.
//!
//! The shape mirrors what `webview.rs` does with the OS webview: one window,
//! tray, close-to-tray, single instance, remembered geometry, toasts. Input goes
//! to the engine through its embedding API; frames come back through the
//! delegate and are presented on a GPU surface the engine owns.

#![cfg(feature = "servo")]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use euclid::Scale;
use servo::{
    Code, ConsoleLogLevel, DevicePoint, EventLoopWaker, InputEvent, JSValue, Key, KeyState,
    KeyboardEvent, LoadStatus, Location, Modifiers, MouseButton as ServoMouseButton,
    MouseButtonAction, MouseButtonEvent, MouseLeftViewportEvent, MouseMoveEvent, NamedKey,
    Notification, Opts, PermissionFeature, PermissionRequest, PrefValue, Preferences,
    RenderingContext, Servo, ServoBuilder, UserContentManager, UserScript, WebView,
    WebViewBuilder, WebViewDelegate, WheelDelta, WheelEvent, WheelMode, WindowRenderingContext,
};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, ModifiersState, PhysicalKey};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::{geometry, notify, paths, single_instance, tray, APP_ID};

const DEFAULT_W: f64 = 1180.0;
const DEFAULT_H: f64 = 860.0;
/// How often to persist geometry and drain single-instance pings.
const TICK: Duration = Duration::from_millis(1500);
/// A line of wheel scrolling, in CSS pixels, for mice that report lines.
const LINE_HEIGHT: f64 = 76.0;

#[derive(Debug)]
enum AppEvent {
    /// Servo has work for its event loop.
    Wake,
    TrayClick,
    MenuOpen,
    MenuQuit,
}

/// Injected before the page's own scripts, for two jobs.
///
/// First, capture errors: WhatsApp swallows most of its own, so the few that
/// escape are the only signal we get about a stalled boot.
///
/// Second, fill in two APIs Servo does not have, both of which a page may call
/// without checking. `requestIdleCallback` is a scheduling convenience and a
/// timeout is a faithful stand-in. `navigator.locks` coordinates tabs that share
/// one origin's storage; this app is a single page in a single process, so there
/// is never another holder and granting immediately is not a shortcut, it is the
/// correct answer for this case. Both are defined only when genuinely absent, so
/// the day Servo ships them the real ones win.
const ERROR_CAPTURE_JS: &str = r#"
(function () {
  if (window.__wa_errors) { return; }
  window.__wa_errors = [];

  if (typeof window.requestIdleCallback !== 'function') {
    window.requestIdleCallback = function (callback, options) {
      var start = Date.now();
      var timeout = (options && options.timeout) || 1;
      return setTimeout(function () {
        callback({
          didTimeout: false,
          timeRemaining: function () { return Math.max(0, 50 - (Date.now() - start)); },
        });
      }, timeout > 50 ? 1 : timeout);
    };
    window.cancelIdleCallback = function (handle) { clearTimeout(handle); };
  }

  if (navigator.locks === undefined) {
    var held = {};
    var lockManager = {
      request: function (name, options, callback) {
        if (typeof options === 'function') { callback = options; options = {}; }
        options = options || {};
        var lock = { name: name, mode: options.mode || 'exclusive' };
        if (options.ifAvailable && held[name]) { return Promise.resolve(callback(null)); }
        var previous = held[name] || Promise.resolve();
        var release;
        held[name] = new Promise(function (resolve) { release = resolve; });
        return previous
          .then(function () { return callback(lock); })
          .then(
            function (value) { release(); return value; },
            function (error) { release(); throw error; }
          );
      },
      query: function () {
        return Promise.resolve({ held: [], pending: [] });
      },
    };
    try {
      Object.defineProperty(navigator, 'locks', { value: lockManager, configurable: true });
    } catch (e) {
      navigator.locks = lockManager;
    }
  }

  var push = function (s) {
    try {
      var text = String(s);
      if (text.length > 400) { text = text.slice(0, 400); }
      window.__wa_errors.push(text);
      if (window.__wa_errors.length > 40) { window.__wa_errors.shift(); }
    } catch (e) {}
  };
  window.addEventListener('error', function (e) {
    push('uncaught: ' + (e.message || '') + ' @ ' + (e.filename || '') + ':' + (e.lineno || 0));
  }, true);
  window.addEventListener('unhandledrejection', function (e) {
    var r = e.reason;
    push('rejected: ' + ((r && (r.stack || r.message)) || r));
  }, true);
})();
"#;

/// Asks the page what it has and what it is showing. Printed on load and on every
/// tick under WHATSAPP_RS_PROBE, which is how a stall gets diagnosed from outside.
const PROBE_JS: &str = r#"
(function () {
  var out = {};
  var missing = [];
  var globals = ['IntersectionObserver', 'ResizeObserver', 'MutationObserver', 'OffscreenCanvas',
    'FontFace', 'BroadcastChannel', 'SharedWorker', 'Worker', 'WebAssembly', 'structuredClone',
    'CompositionEvent', 'requestIdleCallback', 'Notification', 'indexedDB', 'caches', 'Animation'];
  for (var i = 0; i < globals.length; i++) {
    if (typeof window[globals[i]] === 'undefined') { missing.push(globals[i]); }
  }
  if (!navigator.storage) { missing.push('navigator.storage'); }
  else if (!navigator.storage.getDirectory) { missing.push('navigator.storage.getDirectory'); }
  if (!navigator.permissions) { missing.push('navigator.permissions'); }
  if (!navigator.locks) { missing.push('navigator.locks'); }
  if (!navigator.serviceWorker) { missing.push('navigator.serviceWorker'); }
  if (!window.crypto || !window.crypto.subtle) { missing.push('crypto.subtle'); }
  if (!document.body || typeof document.body.animate !== 'function') { missing.push('Element.animate'); }
  out.missing = missing.join(' ');
  out.title = document.title;
  var body = document.body ? (document.body.innerText || '') : '';
  out.screen = body.replace(/\s+/g, ' ').slice(0, 240);
  out.nodes = document.getElementsByTagName('*').length;
  out.errors = (window.__wa_errors || []).slice(-6).join(' || ');
  out.sw = navigator.serviceWorker
    ? (navigator.serviceWorker.controller ? 'controlled' : 'uncontrolled')
    : 'absent';
  return JSON.stringify(out);
})();
"#;

/// Run the probe and print the answer to stderr, tagged with when it ran.
fn probe(webview: &WebView, tag: &'static str) {
    webview.evaluate_javascript(PROBE_JS, move |result| match result {
        Ok(JSValue::String(json)) => eprintln!("[probe {tag}] {json}"),
        Ok(other) => eprintln!("[probe {tag}] unexpected value: {other:?}"),
        Err(err) => eprintln!("[probe {tag}] evaluation failed: {err:?}"),
    });
}

/// One browser identity on every platform. Firefox is on WhatsApp's supported
/// list and Servo is the engine closest to it in lineage; only the OS token
/// changes, and it is the truth.
fn user_agent() -> String {
    let platform = if cfg!(target_os = "windows") {
        "Windows NT 10.0; Win64; x64; rv:143.0"
    } else if cfg!(target_os = "macos") {
        "Macintosh; Intel Mac OS X 10.15; rv:143.0"
    } else {
        "X11; Linux x86_64; rv:143.0"
    };
    format!("Mozilla/5.0 ({platform}) Gecko/20100101 Firefox/143.0")
}

pub(crate) fn run() -> Result<(), Box<dyn std::error::Error>> {
    let listener = match single_instance::acquire() {
        single_instance::Instance::Second => return Ok(()),
        single_instance::Instance::First(l) => l,
    };
    notify::set_app_user_model_id(APP_ID);
    let _ = crate::shortcut::ensure(APP_ID);

    // Servo's network thread uses rustls and panics on its first TLS handshake if
    // no crypto provider was installed by the embedder. Measured 2026-09-07.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let event_loop = EventLoop::<AppEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    install_tray_handlers(&proxy);

    let data_dir = paths::data_dir();
    let mut app = App {
        data_dir: data_dir.clone(),
        geometry: geometry::Store::new(&data_dir),
        waker: Waker(Arc::new(Mutex::new(proxy))),
        listener,
        next_tick: Instant::now() + TICK,
        window: None,
        rendering_context: None,
        servo: None,
        webview: None,
        mouse: Cell::new(DevicePoint::new(-1.0, -1.0)),
        modifiers: ModifiersState::empty(),
        crashed: Rc::new(RefCell::new(None)),
        _tray: None,
        quit_at: std::env::var("WHATSAPP_RS_QUIT_AFTER")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|secs| Instant::now() + Duration::from_secs(secs)),
    };
    event_loop.run_app(&mut app)?;
    if let Some(reason) = app.crashed.borrow().clone() {
        return Err(reason.into());
    }
    Ok(())
}

#[derive(Clone)]
struct Waker(Arc<Mutex<EventLoopProxy<AppEvent>>>);

impl EventLoopWaker for Waker {
    fn wake(&self) {
        let _ = self.0.lock().unwrap().send_event(AppEvent::Wake);
    }
    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(self.clone())
    }
}

/// What the engine tells us about the page.
struct Delegate {
    window: Rc<Window>,
    crashed: Rc<RefCell<Option<String>>>,
}

impl WebViewDelegate for Delegate {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.window.request_redraw();
    }

    fn notify_page_title_changed(&self, _webview: WebView, title: Option<String>) {
        // WhatsApp puts the unread count in its title: "(3) WhatsApp".
        let title = title.filter(|t| !t.trim().is_empty());
        self.window
            .set_title(title.as_deref().unwrap_or("WhatsApp"));
    }

    fn notify_load_status_changed(&self, webview: WebView, status: LoadStatus) {
        eprintln!("[whatsapp-rs] load status: {status:?} url={:?}", webview.url());
        if matches!(status, LoadStatus::Complete) {
            webview.focus();
            if probing() {
                probe(&webview, "load");
            }
            run_eval_hook(&webview);
        }
    }

    /// The page's own console. Without this every error WhatsApp logs is dropped,
    /// which is why a stalled boot looked silent.
    fn show_console_message(&self, _webview: WebView, level: ConsoleLogLevel, message: String) {
        let level = match level {
            ConsoleLogLevel::Error => "error",
            ConsoleLogLevel::Warn => "warn",
            ConsoleLogLevel::Info => "info",
            ConsoleLogLevel::Debug => "debug",
            ConsoleLogLevel::Trace => "trace",
            ConsoleLogLevel::Dir => "dir",
            ConsoleLogLevel::Log => "log",
        };
        eprintln!("[console {level}] {message}");
    }

    fn notify_cursor_changed(&self, _webview: WebView, cursor: servo::Cursor) {
        // Both sides name cursors after the CSS values; winit parses the lowercase form.
        let name = format!("{cursor:?}").to_lowercase();
        if let Ok(icon) = name.parse::<winit::window::CursorIcon>() {
            self.window.set_cursor(icon);
        }
    }

    fn request_permission(&self, _webview: WebView, request: PermissionRequest) {
        // Notifications are the point of the app; persistent storage keeps the
        // message database from being evicted. Everything else is refused, and
        // logged, because a permission this app never expected to be asked for is
        // exactly the kind of thing that stalls a page waiting on the answer.
        let feature = request.feature();
        let allow = matches!(
            feature,
            PermissionFeature::Notifications | PermissionFeature::PersistentStorage
        );
        eprintln!("[whatsapp-rs] permission {feature:?}: {}", if allow { "allowed" } else { "denied" });
        if allow {
            request.allow();
        } else {
            request.deny();
        }
    }

    fn show_notification(&self, _webview: WebView, notification: Notification) {
        notify::toast(&notification.title, &notification.body);
    }

    fn notify_crashed(&self, _webview: WebView, reason: String, _backtrace: Option<String>) {
        *self.crashed.borrow_mut() = Some(format!("The page crashed: {reason}"));
    }
}

struct App {
    data_dir: std::path::PathBuf,
    geometry: geometry::Store,
    waker: Waker,
    listener: std::net::TcpListener,
    next_tick: Instant,
    window: Option<Rc<Window>>,
    rendering_context: Option<Rc<WindowRenderingContext>>,
    servo: Option<Servo>,
    webview: Option<WebView>,
    /// Last known pointer position in device pixels, for button and wheel events.
    mouse: Cell<DevicePoint>,
    modifiers: ModifiersState,
    crashed: Rc<RefCell<Option<String>>>,
    _tray: Option<tray::Tray>,
    /// `WHATSAPP_RS_QUIT_AFTER=<seconds>`: shut down cleanly, exactly as the tray's
    /// Quit does, after this long. The clean path is the one that flushes cookies to
    /// disk, so a persistence test has to use it rather than killing the process.
    quit_at: Option<Instant>,
}

impl App {
    fn start(&mut self, event_loop: &ActiveEventLoop) {
        let saved = self.geometry.load();
        let mut attributes = WindowAttributes::default()
            .with_title("WhatsApp")
            .with_inner_size(LogicalSize::new(DEFAULT_W, DEFAULT_H));
        if let Ok(icon) = winit::window::Icon::from_rgba(tray::ICON_256.to_vec(), 256, 256) {
            attributes = attributes.with_window_icon(Some(icon));
        }
        if let Some(p) = saved {
            attributes = attributes
                .with_inner_size(LogicalSize::new(p.w as f64, p.h as f64))
                .with_position(LogicalPosition::new(p.x as f64, p.y as f64));
        }
        let window = Rc::new(
            event_loop
                .create_window(attributes)
                .expect("create the window"),
        );
        if saved.map(|p| p.maximized).unwrap_or(false) {
            window.set_maximized(true);
        }

        let size = window.inner_size();
        let rendering_context = Rc::new(
            WindowRenderingContext::new(
                event_loop.display_handle().expect("display handle"),
                window.window_handle().expect("window handle"),
                size,
            )
            .expect("create the rendering context"),
        );
        rendering_context
            .make_current()
            .expect("make the rendering context current");

        // Everything WhatsApp needs that Servo keeps off by default, plus where it
        // keeps cookies, local storage, IndexedDB and the cache: our data dir, so a
        // login survives a restart.
        //
        // Servo ships most web APIs behind prefs that default to off, so an app of
        // this size needs them turned on by name. Measured 2026-09-07: with only the
        // first three set, login succeeded and the page then stalled on "Loading your
        // chats" with IntersectionObserver, navigator.storage and navigator.permissions
        // all undefined. A virtualised chat list cannot render without the first.
        let mut preferences = Preferences::default();
        preferences.dom_indexeddb_enabled = true;
        preferences.dom_serviceworker_enabled = true;
        preferences.dom_notification_enabled = true;
        preferences.dom_intersection_observer_enabled = true;
        preferences.dom_storage_manager_api_enabled = true;
        preferences.dom_permissions_enabled = true;
        preferences.dom_offscreen_canvas_enabled = true;
        preferences.dom_web_animations_enabled = true;
        preferences.dom_fontface_enabled = true;
        preferences.dom_visual_viewport_enabled = true;
        preferences.dom_composition_event_enabled = true;
        preferences.dom_entries_api_enabled = true;
        // Web Locks, implemented in our fork. WhatsApp's background worker calls
        // navigator.locks.request() unguarded on its very first line, so without
        // this the worker throws before registering its message handler, never
        // answers the page, and the UI waits on it forever: "Loading your chats".
        preferences.dom_weblocks_enabled = true;
        preferences.user_agent = user_agent();
        let opts = Opts {
            config_dir: Some(self.data_dir.join("servo")),
            ..Default::default()
        };
        let servo = ServoBuilder::default()
            .opts(opts)
            .preferences(preferences)
            .event_loop_waker(Box::new(self.waker.clone()))
            .build();
        // Engine log lines go to stderr under RUST_LOG. A GUI build has no console,
        // so this only shows up when a launcher captures stderr (tools/safe-drive.ps1).
        servo.setup_logging();
        apply_pref_overrides(&servo);

        // The error capture has to be in place before the page's own scripts run.
        let user_content = Rc::new(UserContentManager::new(&servo));
        user_content.add_script(Rc::new(UserScript::new(ERROR_CAPTURE_JS.to_string(), None)));

        let delegate = Rc::new(Delegate {
            window: window.clone(),
            crashed: self.crashed.clone(),
        });
        let url = url::Url::parse(notify::APP_URL).expect("app url");
        let webview = WebViewBuilder::new(&servo, rendering_context.clone() as Rc<dyn RenderingContext>)
            .url(url)
            .hidpi_scale_factor(Scale::new(window.scale_factor() as f32))
            .user_content_manager(user_content)
            .delegate(delegate)
            .build();
        webview.show();
        webview.focus();

        self._tray = tray::build();
        self.window = Some(window);
        self.rendering_context = Some(rendering_context);
        self.servo = Some(servo);
        self.webview = Some(webview);
    }

    fn spin(&mut self) {
        if let Some(servo) = &self.servo {
            servo.spin_event_loop();
        }
    }

    fn show(&self) {
        if let Some(w) = &self.window {
            w.set_visible(true);
            if w.is_minimized().unwrap_or(false) {
                w.set_minimized(false);
            }
            w.focus_window();
        }
    }

    fn hide(&mut self) {
        self.record_geometry();
        if let Some(w) = &self.window {
            w.set_visible(false);
        }
    }

    fn record_geometry(&mut self) {
        let Some(w) = &self.window else { return };
        if w.is_maximized() || w.is_minimized().unwrap_or(false) {
            return;
        }
        let scale = w.scale_factor();
        let size = w.inner_size().to_logical::<f64>(scale);
        let Ok(pos) = w.outer_position() else { return };
        let pos = pos.to_logical::<f64>(scale);
        self.geometry.save_if_changed(geometry::Placement {
            x: pos.x as i32,
            y: pos.y as i32,
            w: size.width as u32,
            h: size.height as u32,
            maximized: false,
        });
    }

    /// The clean shutdown. Dropping the engine runs its own, which is when the
    /// network thread writes the cookie jar to disk; killing the process skips it.
    fn shut_down(&mut self, event_loop: &ActiveEventLoop) {
        self.record_geometry();
        self.webview = None;
        self.servo = None;
        self.rendering_context = None;
        event_loop.exit();
    }

    fn point_from(&self, position: PhysicalPosition<f64>) -> DevicePoint {
        DevicePoint::new(position.x as f32, position.y as f32)
    }

    fn keyboard(&self, event: &KeyEvent, webview: &WebView) {
        let key = match &event.logical_key {
            WinitKey::Character(s) => Key::Character(s.to_string()),
            WinitKey::Named(named) => format!("{named:?}")
                .parse::<NamedKey>()
                .map(Key::Named)
                .unwrap_or(Key::Named(NamedKey::Unidentified)),
            _ => Key::Named(NamedKey::Unidentified),
        };
        let code = match event.physical_key {
            PhysicalKey::Code(c) => format!("{c:?}").parse::<Code>().unwrap_or(Code::Unidentified),
            _ => Code::Unidentified,
        };
        let mut modifiers = Modifiers::empty();
        if self.modifiers.shift_key() {
            modifiers |= Modifiers::SHIFT;
        }
        if self.modifiers.control_key() {
            modifiers |= Modifiers::CONTROL;
        }
        if self.modifiers.alt_key() {
            modifiers |= Modifiers::ALT;
        }
        if self.modifiers.super_key() {
            modifiers |= Modifiers::META;
        }
        let keyboard_event = ::keyboard_types::KeyboardEvent {
            state: match event.state {
                ElementState::Pressed => KeyState::Down,
                ElementState::Released => KeyState::Up,
            },
            key,
            code,
            location: Location::Standard,
            modifiers,
            repeat: event.repeat,
            is_composing: false,
        };
        webview.notify_input_event(InputEvent::Keyboard(KeyboardEvent::new(keyboard_event)));
    }
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            self.start(event_loop);
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::ResumeTimeReached { .. } = cause {
            self.next_tick = Instant::now() + TICK;
            if self.quit_at.is_some_and(|at| Instant::now() >= at) {
                eprintln!("[whatsapp-rs] clean shutdown (WHATSAPP_RS_QUIT_AFTER)");
                self.shut_down(event_loop);
                return;
            }
            if self.window.as_ref().map(|w| w.is_visible().unwrap_or(true)).unwrap_or(false) {
                self.record_geometry();
            }
            if single_instance::poll_show_request(&self.listener) {
                self.show();
            }
            if probing() {
                if let Some(webview) = &self.webview {
                    probe(webview, "tick");
                }
            }
            self.spin();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let (Some(webview), Some(rc)) = (self.webview.clone(), self.rendering_context.clone()) else {
            return;
        };
        match event {
            // The close button and Alt+F4: hide, keep receiving. Quit is on the tray menu.
            WindowEvent::CloseRequested => self.hide(),
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    rc.resize(size);
                    webview.resize(size);
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                webview.set_hidpi_scale_factor(Scale::new(scale_factor as f32));
            }
            WindowEvent::RedrawRequested => {
                webview.paint();
                rc.present();
            }
            WindowEvent::Focused(true) => webview.focus(),
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::KeyboardInput { event, .. } => self.keyboard(&event, &webview),
            WindowEvent::CursorMoved { position, .. } => {
                let point = self.point_from(position);
                self.mouse.set(point);
                webview.notify_input_event(InputEvent::MouseMove(MouseMoveEvent::new(point.into())));
            }
            WindowEvent::CursorLeft { .. } => {
                self.mouse.set(DevicePoint::new(-1.0, -1.0));
                webview.notify_input_event(InputEvent::MouseLeftViewport(
                    MouseLeftViewportEvent::default(),
                ));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    MouseButton::Left => ServoMouseButton::Primary,
                    MouseButton::Right => ServoMouseButton::Secondary,
                    MouseButton::Middle => ServoMouseButton::Auxiliary,
                    MouseButton::Back => ServoMouseButton::Back,
                    MouseButton::Forward => ServoMouseButton::Forward,
                    MouseButton::Other(v) => ServoMouseButton::Other(v),
                };
                let action = match state {
                    ElementState::Pressed => MouseButtonAction::Down,
                    ElementState::Released => MouseButtonAction::Up,
                };
                webview.notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
                    action,
                    button,
                    self.mouse.get().into(),
                )));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (x, y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        (x as f64 * LINE_HEIGHT, y as f64 * LINE_HEIGHT)
                    }
                    MouseScrollDelta::PixelDelta(p) => (p.x, p.y),
                };
                let delta = WheelDelta {
                    x,
                    y,
                    z: 0.0,
                    mode: WheelMode::DeltaPixel,
                };
                webview.notify_input_event(InputEvent::Wheel(WheelEvent::new(
                    delta,
                    self.mouse.get().into(),
                )));
            }
            _ => {}
        }
        self.spin();
        if self.crashed.borrow().is_some() {
            event_loop.exit();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::Wake => {}
            AppEvent::TrayClick => {
                let visible = self
                    .window
                    .as_ref()
                    .and_then(|w| w.is_visible())
                    .unwrap_or(true);
                if visible {
                    self.hide();
                } else {
                    self.show();
                }
            }
            AppEvent::MenuOpen => self.show(),
            AppEvent::MenuQuit => {
                self.shut_down(event_loop);
                return;
            }
        }
        self.spin();
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
    }
}

/// Is the page probe switched on? `WHATSAPP_RS_PROBE=1`.
fn probing() -> bool {
    std::env::var_os("WHATSAPP_RS_PROBE").is_some()
}

/// `WHATSAPP_RS_EVAL="<javascript>"` runs one expression once the page has loaded
/// and prints the result. With `WHATSAPP_RS_QUIT_AFTER=<seconds>` below, this is
/// how storage persistence gets tested: write in one run, read in the next.
fn run_eval_hook(webview: &WebView) {
    let Ok(script) = std::env::var("WHATSAPP_RS_EVAL") else {
        return;
    };
    webview.evaluate_javascript(script, |result| match result {
        Ok(JSValue::String(text)) => eprintln!("[eval] {text}"),
        Ok(other) => eprintln!("[eval] {other:?}"),
        Err(err) => eprintln!("[eval] failed: {err:?}"),
    });
}

/// `WHATSAPP_RS_PREFS="dom_serviceworker_enabled=false,dom_intersection_observer_enabled=true"`
/// changes any engine preference at startup, so a suspect can be switched off and
/// the app re-run without a rebuild. An engine build takes minutes; this takes none.
fn apply_pref_overrides(servo: &Servo) {
    let Ok(spec) = std::env::var("WHATSAPP_RS_PREFS") else {
        return;
    };
    for entry in spec.split(',') {
        let Some((name, raw)) = entry.split_once('=') else {
            continue;
        };
        let (name, raw) = (name.trim(), raw.trim());
        let value = match raw {
            "true" => PrefValue::Bool(true),
            "false" => PrefValue::Bool(false),
            other => match other.parse::<i64>() {
                Ok(n) => PrefValue::Int(n),
                Err(_) => PrefValue::Str(other.to_string()),
            },
        };
        eprintln!("[whatsapp-rs] pref override {name} = {raw}");
        servo.set_preference(name, value);
    }
}

fn install_tray_handlers(proxy: &EventLoopProxy<AppEvent>) {
    {
        let proxy = proxy.clone();
        tray_icon::TrayIconEvent::set_event_handler(Some(move |event| {
            if let tray_icon::TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            } = event
            {
                let _ = proxy.send_event(AppEvent::TrayClick);
            }
        }));
    }
    // The menu ids are only known once the tray exists, but "open" and "quit" are
    // the only two items, so match on the label text muda hands back.
    let proxy = proxy.clone();
    muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
        let id = event.id.0.as_str();
        let _ = proxy.send_event(if id == tray::OPEN_ID {
            AppEvent::MenuOpen
        } else if id == tray::QUIT_ID {
            AppEvent::MenuQuit
        } else {
            return;
        });
    }));
}
