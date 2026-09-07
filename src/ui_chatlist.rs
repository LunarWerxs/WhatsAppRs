//! The chat list: WhatsApp's left column, drawn row by row.
//!
//! A child window of the main one. It reads the rows the main window prepared
//! (`Ui::list`), draws the visible ones, and reports a click back as a selection.

#![cfg(target_os = "windows")]

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    InvalidateRect, DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, DT_CENTER,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::chat_ui::{self, Ui};
use crate::ui_theme as theme;

pub const CLASS: PCWSTR = w!("WhatsAppRsChatList");
/// Not in the windows crate's WindowsAndMessaging set; the SDK value.
const WM_MOUSELEAVE: u32 = 0x02A3;
/// Row height at 96 dpi; WhatsApp's is 72.
pub const ROW_H: i32 = 72;

pub struct Row {
    pub jid: String,
    pub title: String,
    pub preview: String,
    pub time: String,
    pub unread: u32,
    pub is_group: bool,
}

#[derive(Default)]
pub struct ListState {
    pub rows: Vec<Row>,
    pub scroll: i32,
    pub hover: Option<usize>,
    pub tracking: bool,
}

pub unsafe fn register(instance: HINSTANCE) {
    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: instance,
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        lpszClassName: CLASS,
        ..Default::default()
    };
    RegisterClassW(&class);
}

pub unsafe fn create(parent: HWND, instance: HINSTANCE, ui: *mut Ui) -> HWND {
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        CLASS,
        theme::NO_TEXT,
        WS_CHILD | WS_VSCROLL,
        0,
        0,
        0,
        0,
        Some(parent),
        None,
        Some(instance),
        None,
    )
    .expect("create chat list");
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, ui as isize);
    hwnd
}

fn row_h(ui: &Ui) -> i32 {
    ui.px(ROW_H)
}

fn content_h(ui: &Ui) -> i32 {
    ui.list.rows.len() as i32 * row_h(ui)
}

pub unsafe fn update_scrollbar(ui: &mut Ui) {
    let mut rc = RECT::default();
    let _ = GetClientRect(ui.list_hwnd, &mut rc);
    let page = rc.bottom - rc.top;
    let max = (content_h(ui) - page).max(0);
    ui.list.scroll = ui.list.scroll.clamp(0, max);
    chat_ui::set_scrollbar(ui.list_hwnd, content_h(ui), page, ui.list.scroll);
    let _ = InvalidateRect(Some(ui.list_hwnd), None, false);
}

unsafe fn row_at(ui: &Ui, y: i32) -> Option<usize> {
    let i = (y + ui.list.scroll) / row_h(ui);
    if i >= 0 && (i as usize) < ui.list.rows.len() {
        Some(i as usize)
    } else {
        None
    }
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
            update_scrollbar(ui);
            LRESULT(0)
        }
        WM_PAINT => {
            theme::buffered_paint(hwnd, |hdc, rc| paint(ui, hdc, rc));
            LRESULT(0)
        }
        WM_VSCROLL => {
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let page = rc.bottom - rc.top;
            let (line, total) = (row_h(ui), content_h(ui));
            chat_ui::on_vscroll(hwnd, wparam, &mut ui.list.scroll, line, page, total);
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xffff) as u16 as i16 as i32;
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let page = rc.bottom - rc.top;
            let max = (content_h(ui) - page).max(0);
            ui.list.scroll = (ui.list.scroll - delta * row_h(ui) / 120).clamp(0, max);
            chat_ui::set_scrollbar(hwnd, content_h(ui), page, ui.list.scroll);
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let y = ((lparam.0 >> 16) & 0xffff) as u16 as i16 as i32;
            let hover = row_at(ui, y);
            if hover != ui.list.hover {
                ui.list.hover = hover;
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            if !ui.list.tracking {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                if TrackMouseEvent(&mut tme).is_ok() {
                    ui.list.tracking = true;
                }
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            ui.list.tracking = false;
            if ui.list.hover.take().is_some() {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let y = ((lparam.0 >> 16) & 0xffff) as u16 as i16 as i32;
            if let Some(i) = row_at(ui, y) {
                let jid = ui.list.rows[i].jid.clone();
                chat_ui::select_chat(ui, &jid);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn paint(ui: &mut Ui, hdc: windows::Win32::Graphics::Gdi::HDC, rc: RECT) {
    let pal = ui.pal;
    theme::fill(hdc, rc, pal.panel);
    let rh = row_h(ui);
    let width = rc.right - rc.left;
    let first = (ui.list.scroll / rh).max(0) as usize;
    let mut y = -(ui.list.scroll % rh);
    for i in first..ui.list.rows.len() {
        if y > rc.bottom {
            break;
        }
        let row = &ui.list.rows[i];
        let selected = ui.selected.as_deref() == Some(row.jid.as_str());
        let bg = if selected {
            pal.selected
        } else if ui.list.hover == Some(i) {
            pal.hover
        } else {
            pal.panel
        };
        theme::fill(
            hdc,
            RECT {
                left: 0,
                top: y,
                right: width,
                bottom: y + rh,
            },
            bg,
        );

        // Avatar.
        let radius = ui.px(24);
        let cx = ui.px(13) + radius;
        let cy = y + rh / 2;
        match theme::initials(&row.title) {
            Some(text) if !row.is_group => {
                theme::fill_circle(hdc, cx, cy, radius, theme::avatar_color(&row.jid));
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
            _ => theme::draw_silhouette(
                hdc,
                cx,
                cy,
                radius,
                if ui.dark {
                    theme::rgb(0x6B, 0x7C, 0x85)
                } else {
                    theme::rgb(0xDF, 0xE5, 0xE7)
                },
                if ui.dark {
                    theme::rgb(0xCF, 0xD9, 0xDF)
                } else {
                    theme::rgb(0xFF, 0xFF, 0xFF)
                },
            ),
        }

        // Name and time on the first line.
        let text_x = ui.px(76);
        let right = width - ui.px(16);
        let (tw, _) = theme::measure(hdc, &row.time, 400, ui.fonts.small, DT_SINGLELINE);
        let mut time_rc = RECT {
            left: right - tw,
            top: y + ui.px(14),
            right,
            bottom: y + ui.px(34),
        };
        theme::text(
            hdc,
            &row.time,
            &mut time_rc,
            ui.fonts.small,
            if row.unread > 0 { pal.accent } else { pal.text2 },
            DT_RIGHT | DT_SINGLELINE | DT_VCENTER,
        );
        let mut name_rc = RECT {
            left: text_x,
            top: y + ui.px(13),
            right: right - tw - ui.px(8),
            bottom: y + ui.px(35),
        };
        theme::text(
            hdc,
            &row.title,
            &mut name_rc,
            ui.fonts.name,
            pal.text,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
        );

        // Preview and unread badge on the second line.
        let mut preview_right = right;
        if row.unread > 0 {
            let label = row.unread.to_string();
            let (bw, _) = theme::measure(hdc, &label, 200, ui.fonts.small, DT_SINGLELINE);
            let d = ui.px(20);
            let bw = (bw + ui.px(12)).max(d);
            let badge = RECT {
                left: right - bw,
                top: y + ui.px(38),
                right,
                bottom: y + ui.px(38) + d,
            };
            theme::fill_round_rect(hdc, badge, d / 2, pal.badge);
            let mut br = badge;
            theme::text(
                hdc,
                &label,
                &mut br,
                ui.fonts.small,
                pal.badge_text,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            preview_right = badge.left - ui.px(8);
        }
        let mut preview_rc = RECT {
            left: text_x,
            top: y + ui.px(37),
            right: preview_right,
            bottom: y + ui.px(59),
        };
        theme::text(
            hdc,
            &row.preview,
            &mut preview_rc,
            ui.fonts.body,
            pal.text2,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
        );

        // Separator, indented past the avatar like WhatsApp's.
        theme::fill(
            hdc,
            RECT {
                left: text_x,
                top: y + rh - 1,
                right: width,
                bottom: y + rh,
            },
            pal.border,
        );
        y += rh;
    }
}
