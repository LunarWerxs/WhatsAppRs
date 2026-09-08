//! The "Setting up" window shown while the engine unpacks.
//!
//! This is a GUI app with no console, and unpacking 346 MB takes a few seconds on the first
//! run of each version. Without something on screen the user double-clicks an exe and gets
//! nothing at all, which is indistinguishable from a program that failed to start - so they
//! double-click it again, and now two processes are racing to write the same folder.
//!
//! It lives on its own thread with its own message pump, so the caller just unpacks on the
//! thread it is already on and calls [`Window::close`] when it is done. A window belongs to
//! the thread that created it and only that thread may pump for it, which is exactly what
//! this does; the extractor never touches the HWND, it only bumps an `AtomicU64` that the
//! window's timer reads.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// A handle to the running window. Dropping it closes the window.
pub struct Window {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Window {
    /// Show the window and start updating it from `done` out of `total` bytes.
    ///
    /// Never fails the caller: if the window cannot be created for any reason, the unpack
    /// still has to happen, and silently unpacking is better than refusing to start.
    pub fn show(done: Arc<AtomicU64>, total: u64) -> Window {
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("setup-window".into())
                .spawn(move || run(done, total, stop))
                .ok()
        };
        Window { stop, thread }
    }

    pub fn close(mut self) {
        self.shut_down();
    }

    fn shut_down(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        self.shut_down();
    }
}

#[cfg(not(target_os = "windows"))]
fn run(_done: Arc<AtomicU64>, _total: u64, _stop: Arc<AtomicBool>) {}

#[cfg(target_os = "windows")]
fn run(done: Arc<AtomicU64>, total: u64, stop: Arc<AtomicBool>) {
    use std::iter::once;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{CreateFontIndirectW, HBRUSH, HFONT};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Controls::{
        InitCommonControlsEx, ICC_PROGRESS_CLASS, INITCOMMONCONTROLSEX, PBM_SETPOS, PBM_SETRANGE32,
        PBS_SMOOTH, PROGRESS_CLASSW,
    };
    use windows::Win32::UI::HiDpi::GetDpiForSystem;
    use windows::Win32::UI::WindowsAndMessaging::*;

    // The bar's own window class only exists once common controls have been asked for it.
    // The v6 manifest picks *which* comctl32 loads; it does not register the classes.
    unsafe {
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_PROGRESS_CLASS,
        };
        let _ = InitCommonControlsEx(&icc);
    }

    let instance = match unsafe { GetModuleHandleW(None) } {
        Ok(h) => h,
        Err(_) => return,
    };

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            // No PostQuitMessage: the pump below is not a GetMessageW loop and does not end on
            // WM_QUIT. It ends when the unpack sets `stop`, and destroys the window itself.
            WM_DESTROY => LRESULT(0),
            // The static controls paint their own background, and the default is the button
            // face grey against this window's white. Hand them the window colour instead.
            WM_CTLCOLORSTATIC => {
                use windows::Win32::Foundation::COLORREF;
                use windows::Win32::Graphics::Gdi::{
                    GetSysColor, GetSysColorBrush, SetBkColor, SetTextColor, COLOR_WINDOW,
                    COLOR_WINDOWTEXT, HDC,
                };
                let dc = HDC(wparam.0 as _);
                SetBkColor(dc, COLORREF(GetSysColor(COLOR_WINDOW)));
                SetTextColor(dc, COLORREF(GetSysColor(COLOR_WINDOWTEXT)));
                LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize)
            }
            // No close button is offered, but Alt+F4 still arrives here. The unpack owns the
            // window's lifetime; swallowing this is what stops a half-written engine folder.
            WM_CLOSE => LRESULT(0),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    let class = w!("WhatsAppRsSetup");
    let icon = unsafe { LoadIconW(Some(instance.into()), PCWSTR(1 as *const u16)) }.unwrap_or_default();
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        hInstance: instance.into(),
        lpszClassName: class,
        hIcon: icon,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
        hbrBackground: HBRUSH(
            unsafe { windows::Win32::Graphics::Gdi::GetSysColorBrush(
                windows::Win32::Graphics::Gdi::COLOR_WINDOW,
            ) }
            .0,
        ),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&wc) } == 0 {
        // Already registered by an earlier call in this process: fine, carry on.
    }

    // PerMonitorV2 means nothing is scaled for us. 96 is one logical pixel per device pixel.
    let dpi = unsafe { GetDpiForSystem() }.max(96);
    let scale = |n: i32| (n * dpi as i32) / 96;
    let (w, h) = (scale(430), scale(160));
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };

    // WS_SYSMENU is deliberately absent: no close button, because there is nothing sensible
    // to do with a cancelled half-unpack. WS_THICKFRAME too - it is not resizable.
    let hwnd = match unsafe {
        CreateWindowExW(
            WS_EX_APPWINDOW,
            class,
            w!("WhatsApp"),
            WS_OVERLAPPED | WS_CAPTION,
            (screen_w - w) / 2,
            (screen_h - h) / 3,
            w,
            h,
            None,
            None,
            Some(instance.into()),
            None,
        )
    } {
        Ok(hwnd) => hwnd,
        Err(_) => return,
    };

    // The system's own UI font, the one every dialog uses. Without this the controls get the
    // 1995 bitmap font, which is the single most obvious sign of a hand-rolled window.
    let font: HFONT = unsafe {
        let mut metrics = NONCLIENTMETRICSW {
            cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32,
            ..Default::default()
        };
        if SystemParametersInfoW(
            SPI_GETNONCLIENTMETRICS,
            metrics.cbSize,
            Some(&mut metrics as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .is_ok()
        {
            CreateFontIndirectW(&metrics.lfMessageFont)
        } else {
            HFONT::default()
        }
    };

    let label = |text: &str, x: i32, y: i32, cw: i32, ch: i32| {
        let wide: Vec<u16> = text.encode_utf16().chain(once(0)).collect();
        let child = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                PCWSTR(wide.as_ptr()),
                WS_CHILD | WS_VISIBLE,
                scale(x),
                scale(y),
                scale(cw),
                scale(ch),
                Some(hwnd),
                None,
                Some(instance.into()),
                None,
            )
        };
        if let Ok(child) = child {
            if !font.is_invalid() {
                unsafe { SendMessageW(child, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1))) };
            }
        }
    };
    label("Setting up WhatsApp", 18, 16, 390, 20);
    label(
        "Unpacking the browser engine. This happens once per version.",
        18,
        38,
        390,
        20,
    );

    let bar = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PROGRESS_CLASSW,
            PCWSTR::null(),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(PBS_SMOOTH),
            scale(18),
            scale(70),
            scale(390),
            scale(18),
            Some(hwnd),
            None,
            Some(instance.into()),
            None,
        )
    };
    if let Ok(bar) = bar {
        unsafe { SendMessageW(bar, PBM_SETRANGE32, Some(WPARAM(0)), Some(LPARAM(1000))) };
    }

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
    }

    // The pump: drain, update, sleep, check. Deliberately NOT a `GetMessageW` loop driven by a
    // `SetTimer` tick - `SetTimer` can fail (the per-session timer table is finite), and a
    // `GetMessageW` loop whose only wake-up never arrives blocks forever, which would hang
    // `Window::close`'s join and with it the whole startup. This loop's exit condition is the
    // `stop` flag and nothing else.
    let mut message = MSG::default();
    loop {
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        if let Ok(bar) = bar {
            let pos = if total == 0 {
                0
            } else {
                (done.load(Ordering::Relaxed).min(total) * 1000 / total) as usize
            };
            unsafe { SendMessageW(bar, PBM_SETPOS, Some(WPARAM(pos)), None) };
        }
        if stop.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }

    unsafe {
        let _ = DestroyWindow(hwnd);
        // Drain what DestroyWindow posted, so the child controls release the font before it goes.
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            DispatchMessageW(&message);
        }
        if !font.is_invalid() {
            let _ = windows::Win32::Graphics::Gdi::DeleteObject(font.into());
        }
    }
}
