//! The conversation: WhatsApp's bubbles on its patterned-beige (or near-black)
//! background, with date pills between days.
//!
//! A child window. Layout is computed once per width and cached; painting only
//! touches the items inside the visible band.

#![cfg(target_os = "windows")]

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    InvalidateRect, DT_CENTER, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, HDC,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::chat_ui::{self, Ui};
use crate::ui_theme as theme;

pub const CLASS: PCWSTR = w!("WhatsAppRsMessages");

pub enum Item {
    Day {
        y: i32,
        text: String,
    },
    Bubble {
        y: i32,
        h: i32,
        x: i32,
        w: i32,
        index: usize,
        out: bool,
        sender: Option<String>,
        text_h: i32,
    },
}

#[derive(Default)]
pub struct MsgState {
    pub items: Vec<Item>,
    pub content_h: i32,
    pub laid_out_width: i32,
    pub scroll: i32,
    /// Follow new messages while the view is at the bottom, as a chat app should.
    pub stick_bottom: bool,
    pub dirty: bool,
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
    .expect("create message view");
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, ui as isize);
    hwnd
}

/// Recompute positions for the current width, if anything changed.
pub unsafe fn relayout(ui: &mut Ui, hdc: HDC, width: i32) {
    if !ui.msgs.dirty && ui.msgs.laid_out_width == width {
        return;
    }
    ui.msgs.items.clear();
    ui.msgs.laid_out_width = width;
    ui.msgs.dirty = false;

    let Some(jid) = ui.selected.clone() else {
        ui.msgs.content_h = 0;
        return;
    };
    let chats = ui.chats.clone();
    let chats = chats.lock().unwrap();
    let Some(chat) = chats.get(&jid) else {
        ui.msgs.content_h = 0;
        return;
    };

    let side = ui.px(60);
    let max_w = ((width - 2 * side) * 65 / 100).max(ui.px(120));
    let pad_x = ui.px(9);
    let pad_top = ui.px(6);
    let pad_bottom = ui.px(6);
    let gap = ui.px(4);
    let time_h = ui.px(15);
    let name_h = ui.px(18);
    let mut y = ui.px(12);
    let mut last_day: Option<(u16, u16, u16)> = None;

    for (index, m) in chat.messages.iter().enumerate() {
        let day = chat_ui::day_key(m.ts);
        if day != last_day {
            last_day = day;
            ui.msgs.items.push(Item::Day {
                y,
                text: chat_ui::day_label(m.ts),
            });
            y += ui.px(36);
        }
        let sender = if chat.is_group && !m.from_me && !m.sender.is_empty() {
            Some(m.sender.clone())
        } else {
            None
        };
        let inner_w = max_w - 2 * pad_x;
        let (tw, th) = theme::measure(hdc, &m.text, inner_w, ui.fonts.body, DT_LEFT);
        let (time_w, _) = theme::measure(hdc, "00:00", 200, ui.fonts.small, DT_SINGLELINE);
        let (name_w, _) = match &sender {
            Some(s) => theme::measure(hdc, s, inner_w, ui.fonts.name, DT_SINGLELINE),
            None => (0, 0),
        };
        let w = tw.max(time_w + ui.px(20)).max(name_w) + 2 * pad_x;
        let h = pad_top
            + if sender.is_some() { name_h } else { 0 }
            + th
            + time_h
            + pad_bottom;
        let x = if m.from_me { width - side + ui.px(48) - w } else { side - ui.px(48) };
        ui.msgs.items.push(Item::Bubble {
            y,
            h,
            x,
            w,
            index,
            out: m.from_me,
            sender,
            text_h: th,
        });
        y += h + gap;
    }
    ui.msgs.content_h = y + ui.px(8);
}

pub unsafe fn update_scrollbar(ui: &mut Ui) {
    let mut rc = RECT::default();
    let _ = GetClientRect(ui.msgs_hwnd, &mut rc);
    let page = rc.bottom - rc.top;
    let max = (ui.msgs.content_h - page).max(0);
    if ui.msgs.stick_bottom {
        ui.msgs.scroll = max;
    }
    ui.msgs.scroll = ui.msgs.scroll.clamp(0, max);
    chat_ui::set_scrollbar(ui.msgs_hwnd, ui.msgs.content_h, page, ui.msgs.scroll);
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
            ui.msgs.dirty = true;
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_PAINT => {
            theme::buffered_paint(hwnd, |hdc, rc| {
                relayout(ui, hdc, rc.right - rc.left);
                update_scrollbar(ui);
                paint(ui, hdc, rc);
            });
            LRESULT(0)
        }
        WM_VSCROLL => {
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let page = rc.bottom - rc.top;
            let (line, total) = (ui.px(40), ui.msgs.content_h);
            chat_ui::on_vscroll(hwnd, wparam, &mut ui.msgs.scroll, line, page, total);
            ui.msgs.stick_bottom = ui.msgs.scroll >= (ui.msgs.content_h - page).max(0);
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xffff) as u16 as i16 as i32;
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let page = rc.bottom - rc.top;
            let max = (ui.msgs.content_h - page).max(0);
            ui.msgs.scroll = (ui.msgs.scroll - delta * ui.px(40) / 120 * 3).clamp(0, max);
            ui.msgs.stick_bottom = ui.msgs.scroll >= max;
            chat_ui::set_scrollbar(hwnd, ui.msgs.content_h, page, ui.msgs.scroll);
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn paint(ui: &mut Ui, hdc: HDC, rc: RECT) {
    let pal = ui.pal;
    theme::fill(hdc, rc, pal.chat_bg);
    let Some(jid) = ui.selected.clone() else { return };
    let chats = ui.chats.clone();
    let chats = chats.lock().unwrap();
    let Some(chat) = chats.get(&jid) else { return };

    let scroll = ui.msgs.scroll;
    let pad_x = ui.px(9);
    let pad_top = ui.px(6);
    let name_h = ui.px(18);
    let time_h = ui.px(15);
    let width = rc.right - rc.left;

    for item in &ui.msgs.items {
        match item {
            Item::Day { y, text } => {
                let y = y - scroll;
                if y + ui.px(36) < 0 || y > rc.bottom {
                    continue;
                }
                let (tw, _) = theme::measure(hdc, text, 400, ui.fonts.small, DT_SINGLELINE);
                let pw = tw + ui.px(24);
                let pill = RECT {
                    left: (width - pw) / 2,
                    top: y + ui.px(6),
                    right: (width + pw) / 2,
                    bottom: y + ui.px(30),
                };
                theme::fill_round_rect(hdc, pill, ui.px(7), pal.pill);
                let mut tr = pill;
                theme::text(
                    hdc,
                    text,
                    &mut tr,
                    ui.fonts.small,
                    pal.pill_text,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
            }
            Item::Bubble {
                y,
                h,
                x,
                w,
                index,
                out,
                sender,
                text_h,
            } => {
                let y = y - scroll;
                if y + h < 0 || y > rc.bottom {
                    continue;
                }
                let Some(m) = chat.messages.get(*index) else { continue };
                let bubble = RECT {
                    left: *x,
                    top: y,
                    right: x + w,
                    bottom: y + h,
                };
                theme::fill_round_rect(
                    hdc,
                    bubble,
                    ui.px(8),
                    if *out { pal.bubble_out } else { pal.bubble_in },
                );
                let mut cursor = y + pad_top;
                if let Some(name) = sender {
                    let mut nr = RECT {
                        left: x + pad_x,
                        top: cursor,
                        right: x + w - pad_x,
                        bottom: cursor + name_h,
                    };
                    theme::text(
                        hdc,
                        name,
                        &mut nr,
                        ui.fonts.name,
                        theme::avatar_color(name),
                        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
                    );
                    cursor += name_h;
                }
                let mut tr = RECT {
                    left: x + pad_x,
                    top: cursor,
                    right: x + w - pad_x,
                    bottom: cursor + text_h,
                };
                theme::text(hdc, &m.text, &mut tr, ui.fonts.body, pal.text, DT_LEFT | DT_WORDBREAK);
                cursor += text_h;
                let mut time_rc = RECT {
                    left: x + pad_x,
                    top: cursor,
                    right: x + w - pad_x,
                    bottom: cursor + time_h,
                };
                theme::text(
                    hdc,
                    &chat_ui::time_label(m.ts),
                    &mut time_rc,
                    ui.fonts.small,
                    pal.bubble_meta,
                    DT_RIGHT | DT_SINGLELINE | DT_VCENTER,
                );
            }
        }
    }
}
