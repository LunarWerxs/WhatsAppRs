//! Colours, fonts and drawing helpers for light mode's window.
//!
//! The palette is WhatsApp's own (web.whatsapp.com's light and dark themes,
//! sampled 2026-09-07), and which one is in use follows the Windows setting.
//! Shapes go through GDI+ so circles and bubbles are anti-aliased; text stays on
//! GDI, whose ClearType rendering is what every other Windows app shows.

#![cfg(target_os = "windows")]

use std::ffi::c_void;
use std::ptr::null_mut;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, SelectObject, SetBkMode,
    SetTextColor, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH,
    DRAW_TEXT_FORMAT, DT_CALCRECT, DT_NOPREFIX, DT_WORDBREAK, HDC, HFONT, OUT_DEFAULT_PRECIS,
    TRANSPARENT,
};
use windows::Win32::Graphics::GdiPlus::{
    GdipAddPathArc, GdipClosePathFigure, GdipCreateFromHDC, GdipCreatePath, GdipCreateSolidFill,
    GdipDeleteBrush, GdipDeleteGraphics, GdipDeletePath, GdipFillEllipse, GdipFillPath,
    GdipResetClip, GdipSetClipPath, GdipSetSmoothingMode, GdiplusShutdown, GdiplusStartup,
    CombineModeReplace, FillModeAlternate, GdiplusStartupInput, GpBrush, GpGraphics, GpPath,
    GpSolidFill, SmoothingModeAntiAlias,
};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

pub fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((b as u32) << 16 | (g as u32) << 8 | r as u32)
}

fn argb(c: COLORREF) -> u32 {
    let v = c.0;
    0xFF00_0000 | (v & 0xff) << 16 | (v & 0xff00) | (v >> 16) & 0xff
}

/// WhatsApp's colours. Light on the left of each pair, dark on the right.
#[derive(Clone, Copy)]
pub struct Palette {
    pub panel: COLORREF,
    pub header: COLORREF,
    pub border: COLORREF,
    pub text: COLORREF,
    pub text2: COLORREF,
    pub selected: COLORREF,
    pub hover: COLORREF,
    pub badge: COLORREF,
    pub badge_text: COLORREF,
    pub chat_bg: COLORREF,
    pub bubble_out: COLORREF,
    pub bubble_in: COLORREF,
    pub bubble_meta: COLORREF,
    pub composer: COLORREF,
    pub input: COLORREF,
    pub icon: COLORREF,
    pub accent: COLORREF,
    pub pill: COLORREF,
    pub pill_text: COLORREF,
    pub intro_bg: COLORREF,
}

pub fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            panel: rgb(0x11, 0x1B, 0x21),
            header: rgb(0x20, 0x2C, 0x33),
            border: rgb(0x22, 0x2D, 0x34),
            text: rgb(0xE9, 0xED, 0xEF),
            text2: rgb(0x86, 0x96, 0xA0),
            selected: rgb(0x2A, 0x39, 0x42),
            hover: rgb(0x20, 0x2C, 0x33),
            badge: rgb(0x00, 0xA8, 0x84),
            badge_text: rgb(0x11, 0x1B, 0x21),
            chat_bg: rgb(0x0B, 0x14, 0x1A),
            bubble_out: rgb(0x00, 0x5C, 0x4B),
            bubble_in: rgb(0x20, 0x2C, 0x33),
            bubble_meta: rgb(0x8E, 0x9E, 0xA5),
            composer: rgb(0x20, 0x2C, 0x33),
            input: rgb(0x2A, 0x39, 0x42),
            icon: rgb(0x86, 0x96, 0xA0),
            accent: rgb(0x00, 0xA8, 0x84),
            pill: rgb(0x18, 0x22, 0x29),
            pill_text: rgb(0x86, 0x96, 0xA0),
            intro_bg: rgb(0x22, 0x2E, 0x35),
        }
    } else {
        Palette {
            panel: rgb(0xFF, 0xFF, 0xFF),
            header: rgb(0xF0, 0xF2, 0xF5),
            border: rgb(0xE9, 0xED, 0xEF),
            text: rgb(0x11, 0x1B, 0x21),
            text2: rgb(0x66, 0x77, 0x81),
            selected: rgb(0xF0, 0xF2, 0xF5),
            hover: rgb(0xF5, 0xF6, 0xF6),
            badge: rgb(0x25, 0xD3, 0x66),
            badge_text: rgb(0xFF, 0xFF, 0xFF),
            chat_bg: rgb(0xEF, 0xEA, 0xE2),
            bubble_out: rgb(0xD9, 0xFD, 0xD3),
            bubble_in: rgb(0xFF, 0xFF, 0xFF),
            bubble_meta: rgb(0x66, 0x77, 0x81),
            composer: rgb(0xF0, 0xF2, 0xF5),
            input: rgb(0xFF, 0xFF, 0xFF),
            icon: rgb(0x54, 0x65, 0x6F),
            accent: rgb(0x00, 0xA8, 0x84),
            pill: rgb(0xFF, 0xFF, 0xFF),
            pill_text: rgb(0x54, 0x65, 0x6F),
            intro_bg: rgb(0xF0, 0xF2, 0xF5),
        }
    }
}

/// Windows' "Choose your mode" setting for apps. Missing key means light.
/// `WHATSAPP_RS_THEME=light|dark` overrides it, so both looks can be checked
/// without changing the machine's setting.
pub fn system_dark() -> bool {
    match std::env::var("WHATSAPP_RS_THEME").as_deref() {
        Ok("dark") => return true,
        Ok("light") => return false,
        _ => {}
    }
    let mut value: u32 = 1;
    let mut size: u32 = 4;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    status.is_ok() && value == 0
}

pub struct Fonts {
    /// Panel headings: "WhatsApp", the chat name in the header.
    pub title: HFONT,
    /// Chat names in the list, sender names in bubbles.
    pub name: HFONT,
    /// Message text, previews.
    pub body: HFONT,
    /// Times, badges, date pills.
    pub small: HFONT,
    /// Initials inside avatars.
    pub initials: HFONT,
    /// The big line on the empty right-hand panel.
    pub display: HFONT,
}

impl Fonts {
    pub unsafe fn new(dpi: u32) -> Self {
        Fonts {
            title: font(16, 600, dpi),
            name: font(15, 600, dpi),
            body: font(14, 400, dpi),
            small: font(11, 400, dpi),
            initials: font(18, 600, dpi),
            display: font(28, 300, dpi),
        }
    }
    pub unsafe fn free(&self) {
        for f in [
            self.title,
            self.name,
            self.body,
            self.small,
            self.initials,
            self.display,
        ] {
            let _ = DeleteObject(f.into());
        }
    }
}

unsafe fn font(px: i32, weight: i32, dpi: u32) -> HFONT {
    CreateFontW(
        -(px * dpi as i32 / 96),
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        DEFAULT_PITCH.0 as u32,
        w!("Segoe UI"),
    )
}

/// GDI+ lifetime. One per process; shapes are drawn through it.
pub struct GdiPlus(usize);

impl GdiPlus {
    pub unsafe fn start() -> Self {
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            ..Default::default()
        };
        let mut token = 0usize;
        let _ = GdiplusStartup(&mut token, &input, null_mut());
        GdiPlus(token)
    }
    pub unsafe fn stop(&self) {
        GdiplusShutdown(self.0);
    }
}

unsafe fn graphics(hdc: HDC) -> *mut GpGraphics {
    let mut g: *mut GpGraphics = null_mut();
    if GdipCreateFromHDC(hdc, &mut g).0 != 0 {
        return null_mut();
    }
    let _ = GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
    g
}

unsafe fn solid(color: COLORREF) -> *mut GpSolidFill {
    let mut b: *mut GpSolidFill = null_mut();
    let _ = GdipCreateSolidFill(argb(color), &mut b);
    b
}

unsafe fn round_path(r: RECT, radius: i32) -> *mut GpPath {
    let mut path: *mut GpPath = null_mut();
    let _ = GdipCreatePath(FillModeAlternate, &mut path);
    let (x, y) = (r.left as f32, r.top as f32);
    let (w, h) = ((r.right - r.left) as f32, (r.bottom - r.top) as f32);
    let d = ((2 * radius) as f32).min(w).min(h);
    let _ = GdipAddPathArc(path, x, y, d, d, 180.0, 90.0);
    let _ = GdipAddPathArc(path, x + w - d, y, d, d, 270.0, 90.0);
    let _ = GdipAddPathArc(path, x + w - d, y + h - d, d, d, 0.0, 90.0);
    let _ = GdipAddPathArc(path, x, y + h - d, d, d, 90.0, 90.0);
    let _ = GdipClosePathFigure(path);
    path
}

pub unsafe fn fill_round_rect(hdc: HDC, r: RECT, radius: i32, color: COLORREF) {
    let g = graphics(hdc);
    if g.is_null() {
        return;
    }
    let brush = solid(color);
    let path = round_path(r, radius);
    let _ = GdipFillPath(g, brush as *mut GpBrush, path);
    let _ = GdipDeletePath(path);
    let _ = GdipDeleteBrush(brush as *mut GpBrush);
    let _ = GdipDeleteGraphics(g);
}

pub unsafe fn fill_circle(hdc: HDC, cx: i32, cy: i32, radius: i32, color: COLORREF) {
    let g = graphics(hdc);
    if g.is_null() {
        return;
    }
    let brush = solid(color);
    let d = (2 * radius) as f32;
    let _ = GdipFillEllipse(
        g,
        brush as *mut GpBrush,
        (cx - radius) as f32,
        (cy - radius) as f32,
        d,
        d,
    );
    let _ = GdipDeleteBrush(brush as *mut GpBrush);
    let _ = GdipDeleteGraphics(g);
}

/// WhatsApp's default avatar: a person silhouette on a grey disc.
pub unsafe fn draw_silhouette(hdc: HDC, cx: i32, cy: i32, radius: i32, disc: COLORREF, figure: COLORREF) {
    fill_circle(hdc, cx, cy, radius, disc);
    let g = graphics(hdc);
    if g.is_null() {
        return;
    }
    let clip = round_path(
        RECT {
            left: cx - radius,
            top: cy - radius,
            right: cx + radius,
            bottom: cy + radius,
        },
        radius,
    );
    let _ = GdipSetClipPath(g, clip, CombineModeReplace);
    let brush = solid(figure);
    let r = radius as f32;
    let head = r * 0.36;
    let _ = GdipFillEllipse(
        g,
        brush as *mut GpBrush,
        cx as f32 - head,
        cy as f32 - r * 0.62,
        head * 2.0,
        head * 2.0,
    );
    let body_w = r * 1.5;
    let body_h = r * 1.4;
    let _ = GdipFillEllipse(
        g,
        brush as *mut GpBrush,
        cx as f32 - body_w / 2.0,
        cy as f32 + r * 0.22,
        body_w,
        body_h,
    );
    let _ = GdipResetClip(g);
    let _ = GdipDeleteBrush(brush as *mut GpBrush);
    let _ = GdipDeletePath(clip);
    let _ = GdipDeleteGraphics(g);
}

/// The search magnifier: a ring and a handle.
pub unsafe fn draw_magnifier(hdc: HDC, cx: i32, cy: i32, radius: i32, color: COLORREF) {
    use windows::Win32::Graphics::GdiPlus::{
        GdipCreatePen1, GdipDeletePen, GdipDrawEllipse, GdipDrawLine, GpPen, UnitPixel,
    };
    let g = graphics(hdc);
    if g.is_null() {
        return;
    }
    let mut pen: *mut GpPen = null_mut();
    let _ = GdipCreatePen1(argb(color), 1.8, UnitPixel, &mut pen);
    let r = radius as f32;
    let (x, y) = (cx as f32, cy as f32);
    let _ = GdipDrawEllipse(g, pen, x - r, y - r, 2.0 * r, 2.0 * r);
    let _ = GdipDrawLine(g, pen, x + r * 0.72, y + r * 0.72, x + r * 1.7, y + r * 1.7);
    let _ = GdipDeletePen(pen);
    let _ = GdipDeleteGraphics(g);
}

/// The paper-plane send icon, as a filled triangle pair.
pub unsafe fn draw_send_glyph(hdc: HDC, cx: i32, cy: i32, size: i32, color: COLORREF) {
    let g = graphics(hdc);
    if g.is_null() {
        return;
    }
    let brush = solid(color);
    let mut path: *mut GpPath = null_mut();
    let _ = GdipCreatePath(FillModeAlternate, &mut path);
    let s = size as f32;
    let (x, y) = (cx as f32, cy as f32);
    use windows::Win32::Graphics::GdiPlus::GdipAddPathLine;
    // Upper wing.
    let _ = GdipAddPathLine(path, x - s * 0.55, y - s * 0.5, x + s * 0.6, y);
    let _ = GdipAddPathLine(path, x + s * 0.6, y, x - s * 0.25, y + s * 0.05);
    let _ = GdipClosePathFigure(path);
    // Lower wing.
    let _ = GdipAddPathLine(path, x - s * 0.55, y + s * 0.5, x + s * 0.6, y);
    let _ = GdipAddPathLine(path, x + s * 0.6, y, x - s * 0.25, y - s * 0.05);
    let _ = GdipClosePathFigure(path);
    let _ = GdipFillPath(g, brush as *mut GpBrush, path);
    let _ = GdipDeletePath(path);
    let _ = GdipDeleteBrush(brush as *mut GpBrush);
    let _ = GdipDeleteGraphics(g);
}

/// Paint a window through an off-screen bitmap, so nothing flickers.
pub unsafe fn buffered_paint(hwnd: windows::Win32::Foundation::HWND, draw: impl FnOnce(HDC, RECT)) {
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, EndPaint,
        PAINTSTRUCT, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let (w, h) = ((rc.right - rc.left).max(1), (rc.bottom - rc.top).max(1));
    let mem = CreateCompatibleDC(Some(hdc));
    let bmp = CreateCompatibleBitmap(hdc, w, h);
    let old = SelectObject(mem, bmp.into());
    draw(mem, rc);
    let _ = BitBlt(hdc, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
    SelectObject(mem, old);
    let _ = DeleteObject(bmp.into());
    let _ = DeleteDC(mem);
    let _ = EndPaint(hwnd, &ps);
}

pub unsafe fn fill(hdc: HDC, r: RECT, color: COLORREF) {
    let brush = CreateSolidBrush(color);
    FillRect(hdc, &r, brush);
    let _ = DeleteObject(brush.into());
}

/// Draw text and return the height it took.
pub unsafe fn text(
    hdc: HDC,
    s: &str,
    r: &mut RECT,
    font: HFONT,
    color: COLORREF,
    flags: DRAW_TEXT_FORMAT,
) -> i32 {
    if s.is_empty() {
        return 0;
    }
    let old = SelectObject(hdc, font.into());
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, color);
    let mut w: Vec<u16> = s.encode_utf16().collect();
    let h = DrawTextW(hdc, &mut w, r, flags | DT_NOPREFIX);
    SelectObject(hdc, old);
    h
}

/// Width and height the text would take, wrapped at `width`.
pub unsafe fn measure(hdc: HDC, s: &str, width: i32, font: HFONT, flags: DRAW_TEXT_FORMAT) -> (i32, i32) {
    if s.is_empty() {
        return (0, 0);
    }
    let old = SelectObject(hdc, font.into());
    let mut r = RECT {
        left: 0,
        top: 0,
        right: width.max(1),
        bottom: 0,
    };
    let mut w: Vec<u16> = s.encode_utf16().collect();
    DrawTextW(hdc, &mut w, &mut r, flags | DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX);
    SelectObject(hdc, old);
    (r.right - r.left, r.bottom - r.top)
}

/// A stable, pleasant colour per chat, for avatars and group sender names.
pub fn avatar_color(seed: &str) -> COLORREF {
    const COLORS: [(u8, u8, u8); 8] = [
        (0x00, 0xA8, 0x84),
        (0x53, 0xBD, 0xEB),
        (0xE5, 0x42, 0x87),
        (0x91, 0xAB, 0x01),
        (0xFA, 0xA3, 0x1C),
        (0x6B, 0xCB, 0xEF),
        (0xA7, 0x6B, 0xF3),
        (0xFF, 0x7C, 0x3F),
    ];
    let mut h: u32 = 2166136261;
    for b in seed.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    let (r, g, b) = COLORS[(h % COLORS.len() as u32) as usize];
    rgb(r, g, b)
}

/// Up to two initials, or None when the name is a phone number.
pub fn initials(name: &str) -> Option<String> {
    let first = name.trim().chars().next()?;
    if first == '+' || first.is_ascii_digit() {
        return None;
    }
    let mut out = String::new();
    for word in name.split_whitespace().take(2) {
        if let Some(c) = word.chars().next().filter(|c| c.is_alphanumeric()) {
            out.extend(c.to_uppercase());
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

pub const NO_TEXT: PCWSTR = w!("");
