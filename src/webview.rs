//! Safe mode: WhatsApp's real website in the operating system's own webview.
//!
//! One window that owns its own webview, so three whole classes of the C# original
//! disappear: there is no external browser to locate (ChromeFinder), no foreign
//! window to hunt by class name and title (WindowFinder), and no global low-level
//! mouse/keyboard hook to intercept the close button (Hooks). Closing to tray is a
//! single match arm on our own event loop.
//!
//! Engine per platform, all supplied by the OS so nothing is bundled:
//!   Windows -> WebView2      macOS -> WKWebView      Linux -> WebKitGTK

use crate::{geometry, notify, paths, shortcut, single_instance, tray, APP_ID};

use std::time::{Duration, Instant};

use tao::dpi::{LogicalPosition, LogicalSize};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::{Window, WindowBuilder};
use wry::{WebContext, WebViewBuilder};

const DEFAULT_W: f64 = 1180.0;
const DEFAULT_H: f64 = 860.0;
/// How often to check whether the window moved, and to drain single-instance pings.
const TICK: Duration = Duration::from_millis(1500);

#[derive(Debug)]
enum UserEvent {
    TrayClick,
    MenuOpen,
    MenuQuit,
}

pub(crate) fn run() -> wry::Result<()> {
    // Bail out early if we are the second launch; the first one has been told to surface.
    let listener = match single_instance::acquire() {
        single_instance::Instance::Second => return Ok(()),
        single_instance::Instance::First(l) => l,
    };

    // Must precede any toast, or Windows silently declines to render it. The
    // shortcut is the other half of the same identity; without it the ID resolves
    // to nothing and the toast is dropped just as silently.
    notify::set_app_user_model_id(APP_ID);
    let _ = shortcut::ensure(APP_ID);

    let data_dir = paths::data_dir();
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
    let window = window_builder.build(&event_loop).unwrap();
    if saved.map(|p| p.maximized).unwrap_or(false) {
        window.set_maximized(true);
    }

    // The webview profile lives under our data dir, so the whole footprint is one folder.
    let mut web_context = WebContext::new(Some(data_dir.join("profile")));
    let builder = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_url(notify::APP_URL)
        .with_initialization_script(notify::PERMISSION_SHIM_JS);

    // Measured on 2026-09-06 against the stock configuration:
    //   stock          7 processes, 550 MB engine
    //   these flags    2 processes, 399 MB engine
    // Verified that nothing breaks under them: notification permission still granted,
    // service-worker showNotification still displays, NotificationReceived still fires.
    //
    // --single-process is officially unsupported by Chromium and puts the renderer in
    // the browser process, so a renderer crash takes the app down instead of showing a
    // sad-tab. For a single-site viewer of one trusted origin that is an acceptable
    // trade; set WHATSAPP_RS_MULTIPROCESS=1 to fall back if it ever misbehaves.
    #[cfg(target_os = "windows")]
    let builder = {
        use wry::WebViewBuilderExtWindows;
        if std::env::var("WHATSAPP_RS_MULTIPROCESS").is_ok() {
            builder
        } else {
            builder.with_additional_browser_args(concat!(
                "--single-process",
                " --disable-gpu",
                " --disable-extensions",
                " --disable-background-networking",
                " --disable-sync",
                " --disable-breakpad",
                " --disable-component-update",
                " --no-pings",
                " --disable-features=Translate,MediaRouter,OptimizationHints,",
                "OptimizationGuideModelDownloading,InterestFeedContentSuggestions,",
                "CalculateNativeWinOcclusion",
            ))
        }
    };

    let webview = builder.build(&window)?;

    // Both engines deny notifications until the host grants them, and on Windows
    // the host also has to draw the toast itself. See notify.rs.
    notify::grant_notification_permission(&webview);
    notify::bridge_notifications(&webview);

    // tray-icon and muda deliver events on their own channels; forward them into the
    // event loop so everything is handled in one place.
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

    let mut next_tick = Instant::now() + TICK;

    event_loop.run(move |event, _, control_flow| {
        // Wake periodically to persist geometry and drain single-instance pings.
        *control_flow = ControlFlow::WaitUntil(next_tick);

        match event {
            Event::NewEvents(StartCause::Init) => {}

            // The close button AND Alt+F4 both arrive here, which is the whole reason
            // the C# version's global keyboard and mouse hooks are unnecessary.
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                record_geometry(&window, &mut store);
                window.set_visible(false);
            }

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
                if single_instance::poll_show_request(&listener) {
                    show(&window);
                }
            }

            _ => {}
        }
    });
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

/// Only writes when the placement actually changed; see geometry.rs for why.
fn record_geometry(window: &Window, store: &mut geometry::Store) {
    let scale = window.scale_factor();
    let size = window.inner_size().to_logical::<f64>(scale);
    let pos = match window.outer_position() {
        Ok(p) => p.to_logical::<f64>(scale),
        Err(_) => return,
    };
    // A maximized window's restore geometry is not what is on screen, so skip it and
    // keep whatever normal placement was last recorded.
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
