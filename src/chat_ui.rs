//! Light mode's window, in WhatsApp's shape.
//!
//! Left: header, search, the chat list. Right: the chat header, the bubbles, the
//! composer. All of it is drawn by the app with GDI and GDI+ (`ui_theme.rs`), in
//! WhatsApp's own light or dark palette following the Windows setting; the only
//! stock controls are the two text fields, which need a caret and an IME. Until
//! the device is linked the window shows the pairing QR instead. Closing hides to
//! the tray, like safe mode.
//!
//! Why not a toolkit: every GPU-rendered UI layer costs tens of MB just to exist,
//! and the whole point of light mode is the ~20 MB it runs in.

#![cfg(target_os = "windows")]

use std::collections::VecDeque;
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{FILETIME, HINSTANCE, HWND, LPARAM, LRESULT, RECT, SYSTEMTIME, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateSolidBrush, DeleteObject, InvalidateRect, SetBkColor, SetTextColor,
    DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, HBRUSH, HDC,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{SetScrollInfo, EM_SETCUEBANNER};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_RETURN};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::chats::{friendly_jid, Chats};
use crate::light::{self, Outgoing, Sink as LightSink, UiEvent};
use crate::ui_chatlist::{self as chatlist, ListState, Row};
use crate::ui_messages::{self as messages, MsgState};
use crate::ui_theme::{self as theme, Fonts, GdiPlus, Palette};
use crate::{geometry, tray};

const CLASS: PCWSTR = w!("WhatsAppRsLight");
const ID_SEARCH: u16 = 1;
const ID_INPUT: u16 = 2;
/// Posted by the protocol thread after pushing onto the event queue.
const WM_APP_EVENT: u32 = WM_APP + 1;
/// Posted from the tray handlers, which run on this thread inside DispatchMessage.
const WM_APP_TRAY: u32 = WM_APP + 2;
const TRAY_OPEN: usize = 1;
const TRAY_QUIT: usize = 2;
const TRAY_TOGGLE: usize = 3;
const TIMER_SAVE: usize = 1;

// WhatsApp Web's metrics at 96 dpi.
const HEADER_H: i32 = 59;
const SEARCH_H: i32 = 49;
const COMPOSER_H: i32 = 62;
const LEFT_MIN: i32 = 300;
const LEFT_MAX: i32 = 420;

pub enum Screen {
    Connecting,
    Pairing { size: usize, dark: Vec<bool> },
    Chats,
    Stopped(String),
}

/// Where things are this frame; computed in `layout`, read by paint and hit tests.
#[derive(Default, Clone, Copy)]
pub struct Rects {
    pub left_w: i32,
    pub header_h: i32,
    pub search_field: RECT,
    pub composer: RECT,
    pub input_field: RECT,
    pub send_cx: i32,
    pub send_cy: i32,
    pub send_r: i32,
}

pub struct Ui {
    pub hwnd: HWND,
    pub list_hwnd: HWND,
    pub msgs_hwnd: HWND,
    pub search: HWND,
    pub input: HWND,
    pub dpi: u32,
    pub dark: bool,
    pub pal: Palette,
    pub fonts: Fonts,
    /// Backgrounds for the two text fields: the composer's and the search pill's.
    edit_brush: HBRUSH,
    search_brush: HBRUSH,
    gdiplus: GdiPlus,
    pub chats: Arc<Mutex<Chats>>,
    tx: UnboundedSender<Outgoing>,
    queue: Arc<Mutex<VecDeque<UiEvent>>>,
    pub screen: Screen,
    status: String,
    pub selected: Option<String>,
    filter: String,
    pub list: ListState,
    pub msgs: MsgState,
    pub rects: Rects,
    geometry: geometry::Store,
    _tray: Option<tray::Tray>,
    /// Sample chats, no network. Says so in the title the whole time.
    demo: bool,
}

impl Ui {
    /// Device pixels for a 96-dpi measurement.
    pub fn px(&self, v: i32) -> i32 {
        v * self.dpi as i32 / 96
    }
}

/// The protocol thread's handle on the window. Holds the HWND as an integer
/// because the raw pointer type is not Send; PostMessage is thread-safe.
pub struct Sink {
    hwnd: isize,
    queue: Arc<Mutex<VecDeque<UiEvent>>>,
}

impl LightSink for Sink {
    fn event(&self, e: UiEvent) {
        self.queue.lock().unwrap().push_back(e);
        unsafe {
            let _ = PostMessageW(Some(hwnd_of(self.hwnd)), WM_APP_EVENT, WPARAM(0), LPARAM(0));
        }
    }
    fn is_watching(&self) -> bool {
        unsafe { GetForegroundWindow() == hwnd_of(self.hwnd) }
    }
}

fn hwnd_of(raw: isize) -> HWND {
    HWND(raw as *mut c_void)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn run(
    data_dir: PathBuf,
    chats: Arc<Mutex<Chats>>,
    demo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: CLASS,
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err("RegisterClassW failed".into());
        }
        chatlist::register(instance);
        messages::register(instance);

        let mut geometry = geometry::Store::new(&data_dir);
        let (x, y, w, h) = geometry
            .load()
            .map(|p| (p.x, p.y, p.w as i32, p.h as i32))
            .unwrap_or((CW_USEDEFAULT, CW_USEDEFAULT, 1100, 720));
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            w!("WhatsApp"),
            WS_OVERLAPPEDWINDOW,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(instance),
            None,
        )?;

        let dpi = GetDpiForWindow(hwnd).max(96);
        let dark = theme::system_dark();
        let pal = theme::palette(dark);
        let (tx, rx) = unbounded_channel();
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let tray = tray::build();
        install_tray_handlers(hwnd, tray.as_ref());

        let ui = Box::new(Ui {
            hwnd,
            list_hwnd: HWND::default(),
            msgs_hwnd: HWND::default(),
            search: HWND::default(),
            input: HWND::default(),
            dpi,
            dark,
            pal,
            fonts: Fonts::new(dpi),
            edit_brush: CreateSolidBrush(pal.input),
            search_brush: CreateSolidBrush(pal.header),
            gdiplus: GdiPlus::start(),
            chats: chats.clone(),
            tx,
            queue: queue.clone(),
            screen: Screen::Connecting,
            status: String::new(),
            selected: None,
            filter: String::new(),
            list: ListState::default(),
            msgs: MsgState::default(),
            rects: Rects::default(),
            geometry,
            _tray: tray,
            demo,
        });
        let ui_ptr = Box::into_raw(ui);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ui_ptr as isize);
        let ui = &mut *ui_ptr;
        ui.list_hwnd = chatlist::create(hwnd, instance, ui_ptr);
        ui.msgs_hwnd = messages::create(hwnd, instance, ui_ptr);
        ui.search = edit(hwnd, instance, ID_SEARCH, ui.fonts.body, "Search or start a new chat");
        ui.input = edit(hwnd, instance, ID_INPUT, ui.fonts.body, "Type a message");
        let _ = SetWindowSubclass(ui.input, Some(input_proc), 1, ui_ptr as usize);
        apply_dark_title_bar(ui);
        set_icons(ui);
        layout(ui);
        set_title(ui);
        let _ = ShowWindow(hwnd, SW_SHOW);
        // Windows only honours this when the launcher was in front; harmless otherwise.
        let _ = SetForegroundWindow(hwnd);
        SetTimer(Some(hwnd), TIMER_SAVE, 5000, None);

        let sink: Arc<dyn LightSink> = Arc::new(Sink {
            hwnd: hwnd.0 as isize,
            queue,
        });
        let shutdown = Arc::new(tokio::sync::Notify::new());
        let bot = if demo {
            light::spawn_demo(sink, chats.clone(), rx)
        } else {
            light::spawn_bot(data_dir, sink, chats.clone(), rx, shutdown.clone())
        };

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Dropping the window state closes the outgoing channel, which is how the
        // demo thread learns to stop; the real one waits on `shutdown`.
        let ui = Box::from_raw(ui_ptr);
        ui.fonts.free();
        let _ = DeleteObject(ui.edit_brush.into());
        let _ = DeleteObject(ui.search_brush.into());
        ui.gdiplus.stop();
        drop(ui);
        shutdown.notify_one();
        let _ = bot.join();
        chats.lock().unwrap().save_if_due(true);
        Ok(())
    }
}

/// A borderless single-line text field; the frame around it is ours to draw.
unsafe fn edit(
    parent: HWND,
    instance: HINSTANCE,
    id: u16,
    font: windows::Win32::Graphics::Gdi::HFONT,
    cue: &str,
) -> HWND {
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("EDIT"),
        theme::NO_TEXT,
        WS_CHILD | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
        0,
        0,
        0,
        0,
        Some(parent),
        Some(HMENU(id as usize as *mut c_void)),
        Some(instance),
        None,
    )
    .expect("create text field");
    SendMessageW(hwnd, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
    let cue = wide(cue);
    SendMessageW(
        hwnd,
        EM_SETCUEBANNER,
        Some(WPARAM(1)),
        Some(LPARAM(cue.as_ptr() as isize)),
    );
    hwnd
}

/// Enter in the composer sends instead of beeping.
unsafe extern "system" fn input_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    refdata: usize,
) -> LRESULT {
    if msg == WM_KEYDOWN && wparam.0 as u16 == VK_RETURN.0 {
        on_send(&mut *(refdata as *mut Ui));
        return LRESULT(0);
    }
    if msg == WM_CHAR && wparam.0 == 0x0D {
        return LRESULT(0);
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

unsafe fn install_tray_handlers(hwnd: HWND, tray: Option<&tray::Tray>) {
    let raw = hwnd.0 as isize;
    tray_icon::TrayIconEvent::set_event_handler(Some(move |event| {
        if let tray_icon::TrayIconEvent::Click {
            button: tray_icon::MouseButton::Left,
            button_state: tray_icon::MouseButtonState::Up,
            ..
        } = event
        {
            let _ = PostMessageW(Some(hwnd_of(raw)), WM_APP_TRAY, WPARAM(TRAY_TOGGLE), LPARAM(0));
        }
    }));
    if let Some(t) = tray {
        let open_id = t.open_id.clone();
        let quit_id = t.quit_id.clone();
        muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
            let which = if event.id == open_id {
                TRAY_OPEN
            } else if event.id == quit_id {
                TRAY_QUIT
            } else {
                return;
            };
            let _ = PostMessageW(Some(hwnd_of(raw)), WM_APP_TRAY, WPARAM(which), LPARAM(0));
        }));
    }
}

/// The title bar follows the theme too; without this it stays white in dark mode.
unsafe fn apply_dark_title_bar(ui: &Ui) {
    let value: i32 = if ui.dark { 1 } else { 0 };
    let _ = DwmSetWindowAttribute(
        ui.hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
        &value as *const i32 as *const c_void,
        4,
    );
}

unsafe fn set_icons(ui: &Ui) {
    for (bytes, size, which) in [(tray::ICON_32, 32, ICON_SMALL), (tray::ICON_256, 256, ICON_BIG)] {
        if let Some(icon) = icon_from_rgba(bytes, size) {
            SendMessageW(
                ui.hwnd,
                WM_SETICON,
                Some(WPARAM(which as usize)),
                Some(LPARAM(icon.0 as isize)),
            );
        }
    }
}

/// The embedded RGBA icon as an HICON: BGRA, premultiplied, 32 bits.
unsafe fn icon_from_rgba(rgba: &[u8], size: i32) -> Option<HICON> {
    if rgba.len() != (size * size * 4) as usize {
        return None;
    }
    let mut bgra = Vec::with_capacity(rgba.len());
    for p in rgba.chunks(4) {
        let a = p[3] as u32;
        bgra.push((p[2] as u32 * a / 255) as u8);
        bgra.push((p[1] as u32 * a / 255) as u8);
        bgra.push((p[0] as u32 * a / 255) as u8);
        bgra.push(p[3]);
    }
    let color = CreateBitmap(size, size, 1, 32, Some(bgra.as_ptr() as *const c_void));
    let mask = CreateBitmap(size, size, 1, 1, None);
    let info = ICONINFO {
        fIcon: true.into(),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: color,
    };
    let icon = CreateIconIndirect(&info).ok();
    let _ = DeleteObject(color.into());
    let _ = DeleteObject(mask.into());
    icon
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ui;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ui = &mut *ptr;
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_SIZE => {
            layout(ui);
            LRESULT(0)
        }
        WM_PAINT => {
            theme::buffered_paint(hwnd, |hdc, rc| paint(ui, hdc, rc));
            LRESULT(0)
        }
        WM_CTLCOLOREDIT => {
            let hdc = HDC(wparam.0 as *mut c_void);
            let is_search = HWND(lparam.0 as *mut c_void) == ui.search;
            SetBkColor(hdc, if is_search { ui.pal.header } else { ui.pal.input });
            SetTextColor(hdc, ui.pal.text);
            LRESULT(if is_search { ui.search_brush.0 } else { ui.edit_brush.0 } as isize)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xffff) as u16;
            let code = ((wparam.0 >> 16) & 0xffff) as u32;
            if id == ID_SEARCH && code == EN_CHANGE {
                ui.filter = window_text(ui.search).trim().to_lowercase();
                rebuild_rows(ui);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xffff) as u16 as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xffff) as u16 as i16 as i32;
            let r = ui.rects;
            let (dx, dy) = (x - r.send_cx, y - r.send_cy);
            if ui.selected.is_some() && dx * dx + dy * dy <= r.send_r * r.send_r {
                on_send(ui);
            } else if matches!(ui.screen, Screen::Chats) {
                let _ = SetFocus(Some(ui.input));
            }
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            if theme::system_dark() != ui.dark {
                apply_theme(ui);
            }
            LRESULT(0)
        }
        WM_APP_EVENT => {
            drain(ui);
            LRESULT(0)
        }
        WM_APP_TRAY => {
            match wparam.0 {
                TRAY_OPEN => show(ui),
                TRAY_TOGGLE => {
                    if IsWindowVisible(hwnd).as_bool() {
                        hide(ui);
                    } else {
                        show(ui);
                    }
                }
                TRAY_QUIT => {
                    record_geometry(ui);
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_TIMER => {
            ui.chats.lock().unwrap().save_if_due(false);
            LRESULT(0)
        }
        // The X button and Alt+F4: hide, keep receiving. Quit is on the tray menu.
        WM_CLOSE => {
            hide(ui);
            LRESULT(0)
        }
        WM_SETFOCUS => {
            if matches!(ui.screen, Screen::Chats) && ui.selected.is_some() {
                let _ = SetFocus(Some(ui.input));
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn window_text(hwnd: HWND) -> String {
    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let got = GetWindowTextW(hwnd, &mut buf);
    String::from_utf16_lossy(&buf[..got.max(0) as usize])
}

unsafe fn show(ui: &mut Ui) {
    let _ = ShowWindow(ui.hwnd, SW_SHOW);
    if IsIconic(ui.hwnd).as_bool() {
        let _ = ShowWindow(ui.hwnd, SW_RESTORE);
    }
    let _ = SetForegroundWindow(ui.hwnd);
}

unsafe fn hide(ui: &mut Ui) {
    record_geometry(ui);
    let _ = ShowWindow(ui.hwnd, SW_HIDE);
}

/// Only writes when the placement actually changed; see geometry.rs.
unsafe fn record_geometry(ui: &mut Ui) {
    if IsIconic(ui.hwnd).as_bool() || IsZoomed(ui.hwnd).as_bool() {
        return;
    }
    let mut r = RECT::default();
    if GetWindowRect(ui.hwnd, &mut r).is_err() {
        return;
    }
    ui.geometry.save_if_changed(geometry::Placement {
        x: r.left,
        y: r.top,
        w: (r.right - r.left).max(0) as u32,
        h: (r.bottom - r.top).max(0) as u32,
        maximized: false,
    });
}

unsafe fn set_title(ui: &Ui) {
    let mut title = match &ui.screen {
        Screen::Chats if ui.status.is_empty() => "WhatsApp".to_string(),
        Screen::Pairing { .. } => "WhatsApp: scan to link this device".to_string(),
        _ if ui.status.is_empty() => "WhatsApp".to_string(),
        _ => format!("WhatsApp: {}", ui.status),
    };
    if ui.demo {
        title.push_str(" (demo, not connected)");
    }
    let _ = SetWindowTextW(ui.hwnd, PCWSTR(wide(&title).as_ptr()));
}

unsafe fn apply_theme(ui: &mut Ui) {
    ui.dark = theme::system_dark();
    ui.pal = theme::palette(ui.dark);
    let _ = DeleteObject(ui.edit_brush.into());
    let _ = DeleteObject(ui.search_brush.into());
    ui.edit_brush = CreateSolidBrush(ui.pal.input);
    ui.search_brush = CreateSolidBrush(ui.pal.header);
    apply_dark_title_bar(ui);
    for h in [ui.hwnd, ui.list_hwnd, ui.msgs_hwnd, ui.search, ui.input] {
        let _ = InvalidateRect(Some(h), None, true);
    }
}

unsafe fn layout(ui: &mut Ui) {
    let mut rc = RECT::default();
    let _ = GetClientRect(ui.hwnd, &mut rc);
    let (cw, ch) = (rc.right - rc.left, rc.bottom - rc.top);
    let chats = matches!(ui.screen, Screen::Chats);
    let has_chat = chats && ui.selected.is_some();

    let left_w = (cw * 30 / 100).clamp(ui.px(LEFT_MIN), ui.px(LEFT_MAX)).min(cw / 2);
    let header_h = ui.px(HEADER_H);
    let search_h = ui.px(SEARCH_H);
    let composer_h = ui.px(COMPOSER_H);
    let mut r = Rects {
        left_w,
        header_h,
        ..Default::default()
    };

    r.search_field = RECT {
        left: ui.px(12),
        top: header_h + ui.px(7),
        right: left_w - ui.px(12),
        bottom: header_h + ui.px(7) + ui.px(35),
    };
    r.composer = RECT {
        left: left_w + 1,
        top: ch - composer_h,
        right: cw,
        bottom: ch,
    };
    r.send_r = ui.px(22);
    r.send_cx = cw - ui.px(16) - r.send_r;
    r.send_cy = r.composer.top + composer_h / 2;
    r.input_field = RECT {
        left: r.composer.left + ui.px(16),
        top: r.composer.top + ui.px(10),
        right: r.send_cx - r.send_r - ui.px(12),
        bottom: r.composer.bottom - ui.px(10),
    };
    ui.rects = r;

    let _ = ShowWindow(ui.list_hwnd, if chats { SW_SHOW } else { SW_HIDE });
    let _ = ShowWindow(ui.search, if chats { SW_SHOW } else { SW_HIDE });
    let _ = ShowWindow(ui.msgs_hwnd, if has_chat { SW_SHOW } else { SW_HIDE });
    let _ = ShowWindow(ui.input, if has_chat { SW_SHOW } else { SW_HIDE });
    if chats {
        let _ = MoveWindow(
            ui.list_hwnd,
            0,
            header_h + search_h,
            left_w,
            (ch - header_h - search_h).max(0),
            true,
        );
        let field_h = ui.px(19);
        let f = r.search_field;
        let _ = MoveWindow(
            ui.search,
            f.left + ui.px(44),
            f.top + (f.bottom - f.top - field_h) / 2,
            (f.right - f.left - ui.px(56)).max(10),
            field_h,
            true,
        );
    }
    if has_chat {
        let _ = MoveWindow(
            ui.msgs_hwnd,
            left_w + 1,
            header_h,
            (cw - left_w - 1).max(0),
            (ch - header_h - composer_h).max(0),
            true,
        );
        let field_h = ui.px(19);
        let f = r.input_field;
        let _ = MoveWindow(
            ui.input,
            f.left + ui.px(14),
            f.top + (f.bottom - f.top - field_h) / 2,
            (f.right - f.left - ui.px(28)).max(10),
            field_h,
            true,
        );
    }
    let _ = InvalidateRect(Some(ui.hwnd), None, false);
}

unsafe fn paint(ui: &mut Ui, hdc: HDC, rc: RECT) {
    if !matches!(ui.screen, Screen::Chats) {
        paint_screen(ui, hdc, rc);
        return;
    }
    let pal = ui.pal;
    let r = ui.rects;
    let (cw, ch) = (rc.right - rc.left, rc.bottom - rc.top);

    // Left header: your avatar on the panel colour.
    theme::fill(
        hdc,
        RECT {
            left: 0,
            top: 0,
            right: r.left_w,
            bottom: r.header_h,
        },
        pal.header,
    );
    let (disc, figure) = silhouette_colors(ui);
    theme::draw_silhouette(hdc, ui.px(16) + ui.px(20), r.header_h / 2, ui.px(20), disc, figure);
    let mut title_rc = RECT {
        left: ui.px(66),
        top: 0,
        right: r.left_w - ui.px(12),
        bottom: r.header_h,
    };
    theme::text(
        hdc,
        "WhatsApp",
        &mut title_rc,
        ui.fonts.title,
        pal.text,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
    );

    // Search field.
    theme::fill(
        hdc,
        RECT {
            left: 0,
            top: r.header_h,
            right: r.left_w,
            bottom: r.header_h + ui.px(SEARCH_H),
        },
        pal.panel,
    );
    theme::fill_round_rect(hdc, r.search_field, ui.px(8), pal.header);
    theme::draw_magnifier(
        hdc,
        r.search_field.left + ui.px(22),
        (r.search_field.top + r.search_field.bottom) / 2 - ui.px(1),
        ui.px(6),
        pal.icon,
    );
    theme::fill(
        hdc,
        RECT {
            left: 0,
            top: r.header_h + ui.px(SEARCH_H) - 1,
            right: r.left_w,
            bottom: r.header_h + ui.px(SEARCH_H),
        },
        pal.border,
    );

    // The divider.
    theme::fill(
        hdc,
        RECT {
            left: r.left_w,
            top: 0,
            right: r.left_w + 1,
            bottom: ch,
        },
        pal.border,
    );

    let right = RECT {
        left: r.left_w + 1,
        top: 0,
        right: cw,
        bottom: ch,
    };
    let Some(jid) = ui.selected.clone() else {
        paint_intro(ui, hdc, right);
        return;
    };

    // Chat header.
    theme::fill(
        hdc,
        RECT {
            bottom: r.header_h,
            ..right
        },
        pal.header,
    );
    let (title, is_group, subtitle) = {
        let chats = ui.chats.lock().unwrap();
        match chats.get(&jid) {
            Some(c) => (
                c.title(),
                c.is_group,
                if c.is_group {
                    "Group".to_string()
                } else {
                    friendly_jid(&c.jid)
                },
            ),
            None => (friendly_jid(&jid), false, String::new()),
        }
    };
    let cx = right.left + ui.px(16) + ui.px(20);
    let cy = r.header_h / 2;
    draw_avatar(ui, hdc, cx, cy, ui.px(20), &title, &jid, is_group);
    let mut name_rc = RECT {
        left: right.left + ui.px(66),
        top: ui.px(11),
        right: right.right - ui.px(16),
        bottom: ui.px(33),
    };
    theme::text(
        hdc,
        &title,
        &mut name_rc,
        ui.fonts.name,
        pal.text,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
    );
    let mut sub_rc = RECT {
        top: ui.px(32),
        bottom: ui.px(50),
        ..name_rc
    };
    theme::text(
        hdc,
        &subtitle,
        &mut sub_rc,
        ui.fonts.small,
        pal.text2,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
    );
    theme::fill(
        hdc,
        RECT {
            top: r.header_h - 1,
            bottom: r.header_h,
            ..right
        },
        pal.border,
    );

    // Composer.
    theme::fill(hdc, r.composer, pal.composer);
    theme::fill_round_rect(hdc, r.input_field, ui.px(8), pal.input);
    theme::fill_circle(hdc, r.send_cx, r.send_cy, r.send_r, pal.accent);
    theme::draw_send_glyph(
        hdc,
        r.send_cx + ui.px(1),
        r.send_cy,
        ui.px(18),
        theme::rgb(0xFF, 0xFF, 0xFF),
    );
}

fn silhouette_colors(ui: &Ui) -> (windows::Win32::Foundation::COLORREF, windows::Win32::Foundation::COLORREF) {
    if ui.dark {
        (theme::rgb(0x6B, 0x7C, 0x85), theme::rgb(0xCF, 0xD9, 0xDF))
    } else {
        (theme::rgb(0xDF, 0xE5, 0xE7), theme::rgb(0xFF, 0xFF, 0xFF))
    }
}

pub unsafe fn draw_avatar(ui: &Ui, hdc: HDC, cx: i32, cy: i32, radius: i32, title: &str, jid: &str, is_group: bool) {
    match theme::initials(title) {
        Some(text) if !is_group => {
            theme::fill_circle(hdc, cx, cy, radius, theme::avatar_color(jid));
            let mut r = RECT {
                left: cx - radius,
                top: cy - radius,
                right: cx + radius,
                bottom: cy + radius,
            };
            theme::text(
                hdc,
                &text,
                &mut r,
                ui.fonts.initials,
                theme::rgb(0xFF, 0xFF, 0xFF),
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
        _ => {
            let (disc, figure) = silhouette_colors(ui);
            theme::draw_silhouette(hdc, cx, cy, radius, disc, figure);
        }
    }
}

/// The right-hand panel before a chat is picked.
unsafe fn paint_intro(ui: &Ui, hdc: HDC, r: RECT) {
    let pal = ui.pal;
    theme::fill(hdc, r, pal.intro_bg);
    theme::fill(
        hdc,
        RECT {
            bottom: r.top + ui.px(6),
            ..r
        },
        pal.accent,
    );
    let mid = (r.top + r.bottom) / 2;
    let mut big = RECT {
        left: r.left + ui.px(40),
        top: mid - ui.px(40),
        right: r.right - ui.px(40),
        bottom: mid + ui.px(10),
    };
    theme::text(
        hdc,
        "WhatsApp for Windows",
        &mut big,
        ui.fonts.display,
        pal.text,
        DT_CENTER | DT_SINGLELINE | DT_VCENTER,
    );
    let mut small = RECT {
        left: r.left + ui.px(60),
        top: mid + ui.px(14),
        right: r.right - ui.px(60),
        bottom: mid + ui.px(70),
    };
    theme::text(
        hdc,
        "Send and receive messages without keeping your phone online.\n\
         Light mode: one small process, no browser.",
        &mut small,
        ui.fonts.body,
        pal.text2,
        DT_CENTER | DT_WORDBREAK,
    );
}

/// Connecting, pairing (the QR) and stopped screens.
unsafe fn paint_screen(ui: &Ui, hdc: HDC, rc: RECT) {
    let pal = ui.pal;
    theme::fill(hdc, rc, pal.panel);
    let text = match &ui.screen {
        Screen::Connecting => {
            if ui.status.is_empty() {
                "Connecting to WhatsApp.".to_string()
            } else {
                ui.status.clone()
            }
        }
        Screen::Pairing { .. } => "Open WhatsApp on your phone, go to Settings, Linked devices, \
             Link a device, and scan this code. It expires after about a minute; a fresh \
             one appears by itself."
            .to_string(),
        Screen::Stopped(reason) => reason.clone(),
        Screen::Chats => String::new(),
    };
    let mut text_rc = RECT {
        left: rc.left + ui.px(60),
        top: rc.top + ui.px(32),
        right: rc.right - ui.px(60),
        bottom: rc.top + ui.px(100),
    };
    theme::text(hdc, &text, &mut text_rc, ui.fonts.body, pal.text, DT_CENTER | DT_WORDBREAK);

    if let Screen::Pairing { size, dark } = &ui.screen {
        let size = *size as i32;
        let top = text_rc.bottom + ui.px(16);
        let avail = (rc.right - rc.left).min(rc.bottom - top) - ui.px(60);
        let module = (avail / (size + 4)).max(1);
        let total = module * size;
        let x0 = rc.left + ((rc.right - rc.left) - total) / 2;
        let y0 = top + ((rc.bottom - top) - total) / 2;
        let margin = module * 2;
        theme::fill_round_rect(
            hdc,
            RECT {
                left: x0 - margin,
                top: y0 - margin,
                right: x0 + total + margin,
                bottom: y0 + total + margin,
            },
            ui.px(12),
            theme::rgb(0xFF, 0xFF, 0xFF),
        );
        let black = theme::rgb(0, 0, 0);
        for (i, is_dark) in dark.iter().enumerate() {
            if !is_dark {
                continue;
            }
            let (col, row) = ((i as i32) % size, (i as i32) / size);
            theme::fill(
                hdc,
                RECT {
                    left: x0 + col * module,
                    top: y0 + row * module,
                    right: x0 + (col + 1) * module,
                    bottom: y0 + (row + 1) * module,
                },
                black,
            );
        }
    }
}

unsafe fn drain(ui: &mut Ui) {
    loop {
        let event = ui.queue.lock().unwrap().pop_front();
        let Some(event) = event else { break };
        match event {
            UiEvent::Qr(code) => {
                ui.screen = match qrcode::QrCode::new(code.as_bytes()) {
                    Ok(qr) => Screen::Pairing {
                        size: qr.width(),
                        dark: qr
                            .to_colors()
                            .into_iter()
                            .map(|c| c == qrcode::Color::Dark)
                            .collect(),
                    },
                    Err(e) => Screen::Stopped(format!("Could not render the pairing code: {e}")),
                };
                layout(ui);
                set_title(ui);
            }
            UiEvent::Connected => {
                ui.screen = Screen::Chats;
                ui.status.clear();
                layout(ui);
                set_title(ui);
                rebuild_rows(ui);
            }
            UiEvent::LoggedOut(reason) => {
                ui.screen = Screen::Stopped(format!(
                    "This device was logged out ({reason}). Delete light-session.db in the \
                     data folder and start the app again to link it afresh."
                ));
                layout(ui);
                set_title(ui);
            }
            UiEvent::ChatsChanged => {
                if matches!(ui.screen, Screen::Chats) {
                    // The open chat is being read, so nothing in it counts as unread.
                    if let Some(sel) = &ui.selected {
                        if IsWindowVisible(ui.hwnd).as_bool() {
                            ui.chats.lock().unwrap().mark_read(sel);
                        }
                    }
                    rebuild_rows(ui);
                    ui.msgs.dirty = true;
                    let _ = InvalidateRect(Some(ui.msgs_hwnd), None, false);
                }
            }
            UiEvent::Status(s) => {
                ui.status = s;
                if let Screen::Connecting = ui.screen {
                    let _ = InvalidateRect(Some(ui.hwnd), None, true);
                }
                set_title(ui);
            }
        }
    }
}

/// Rows for the list, from the store, through the search filter.
pub unsafe fn rebuild_rows(ui: &mut Ui) {
    let rows: Vec<Row> = {
        let chats = ui.chats.lock().unwrap();
        chats
            .sorted()
            .into_iter()
            .filter(|c| ui.filter.is_empty() || c.title().to_lowercase().contains(&ui.filter))
            .map(|c| {
                let preview = match c.messages.last() {
                    Some(m) => {
                        let body = m.text.replace(['\r', '\n'], " ");
                        if m.from_me {
                            format!("You: {body}")
                        } else if c.is_group && !m.sender.is_empty() {
                            format!("{}: {body}", m.sender)
                        } else {
                            body
                        }
                    }
                    None => String::new(),
                };
                Row {
                    jid: c.jid.clone(),
                    title: c.title(),
                    preview,
                    time: row_time(c.last_ts),
                    unread: c.unread,
                    is_group: c.is_group,
                }
            })
            .collect()
    };
    ui.list.rows = rows;
    chatlist::update_scrollbar(ui);
}

pub unsafe fn select_chat(ui: &mut Ui, jid: &str) {
    ui.chats.lock().unwrap().mark_read(jid);
    ui.selected = Some(jid.to_string());
    ui.msgs.dirty = true;
    ui.msgs.stick_bottom = true;
    rebuild_rows(ui);
    layout(ui);
    let _ = InvalidateRect(Some(ui.msgs_hwnd), None, false);
    let _ = SetFocus(Some(ui.input));
}

unsafe fn on_send(ui: &mut Ui) {
    let Some(jid) = ui.selected.clone() else { return };
    let text = window_text(ui.input);
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let _ = ui.tx.send(Outgoing {
        jid,
        text: text.to_string(),
    });
    let _ = SetWindowTextW(ui.input, theme::NO_TEXT);
    let _ = SetFocus(Some(ui.input));
}

// Scrollbar plumbing shared by both panels.

pub unsafe fn set_scrollbar(hwnd: HWND, content_h: i32, page: i32, pos: i32) {
    let si = SCROLLINFO {
        cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
        fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
        nMin: 0,
        nMax: (content_h - 1).max(0),
        nPage: page.max(0) as u32,
        nPos: pos,
        nTrackPos: 0,
    };
    SetScrollInfo(hwnd, SB_VERT, &si, true);
}

pub unsafe fn on_vscroll(hwnd: HWND, wparam: WPARAM, scroll: &mut i32, line: i32, page: i32, content_h: i32) {
    let max = (content_h - page).max(0);
    let request = (wparam.0 & 0xffff) as i32;
    let mut si = SCROLLINFO {
        cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
        fMask: SIF_TRACKPOS,
        ..Default::default()
    };
    let _ = GetScrollInfo(hwnd, SB_VERT, &mut si);
    let next = match SCROLLBAR_COMMAND(request) {
        SB_LINEUP => *scroll - line,
        SB_LINEDOWN => *scroll + line,
        SB_PAGEUP => *scroll - page,
        SB_PAGEDOWN => *scroll + page,
        SB_THUMBTRACK | SB_THUMBPOSITION => si.nTrackPos,
        SB_TOP => 0,
        SB_BOTTOM => max,
        _ => *scroll,
    };
    *scroll = next.clamp(0, max);
    set_scrollbar(hwnd, content_h, page, *scroll);
}

// Local time. Windows does the zone and DST arithmetic; the chrono that comes
// with whatsapp-rust is built without its clock feature.

fn local_time(ts: u64) -> Option<SYSTEMTIME> {
    use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    const EPOCH_DIFF: u64 = 11_644_473_600;
    let ticks = (ts + EPOCH_DIFF) * 10_000_000;
    let ft = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    unsafe {
        FileTimeToSystemTime(&ft, &mut utc).ok()?;
        SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).ok()?;
    }
    Some(local)
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn day_key(ts: u64) -> Option<(u16, u16, u16)> {
    local_time(ts).map(|t| (t.wYear, t.wMonth, t.wDay))
}

/// "14:05".
pub fn time_label(ts: u64) -> String {
    local_time(ts)
        .map(|t| format!("{:02}:{:02}", t.wHour, t.wMinute))
        .unwrap_or_default()
}

/// "Today", "Yesterday", or "07/09/2026", for the pills between days.
pub fn day_label(ts: u64) -> String {
    let today = day_key(now());
    let yesterday = day_key(now().saturating_sub(86_400));
    let key = day_key(ts);
    if key.is_some() && key == today {
        "Today".into()
    } else if key.is_some() && key == yesterday {
        "Yesterday".into()
    } else {
        local_time(ts)
            .map(|t| format!("{:02}/{:02}/{}", t.wDay, t.wMonth, t.wYear))
            .unwrap_or_default()
    }
}

/// The time column in the chat list: the time today, otherwise the day.
fn row_time(ts: u64) -> String {
    if ts == 0 {
        return String::new();
    }
    if day_key(ts) == day_key(now()) {
        time_label(ts)
    } else {
        day_label(ts)
    }
}
