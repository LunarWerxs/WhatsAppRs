//! Which engine the app runs on, and the first-run choice between them.
//!
//! The two modes are not two settings of one thing. They are genuinely different
//! programs sharing a launcher:
//!
//! - [`Mode::Safe`] loads `web.whatsapp.com` in the operating system's own webview.
//!   That is what a browser does. WhatsApp's Terms of Service say nothing about
//!   which browser you use, so this breaks no rule and carries no account risk.
//!   Measured: about 375 MB.
//!
//! - [`Mode::Light`] speaks WhatsApp's protocol directly, with no browser at all.
//!   Measured: about 12 MB, one process. That protocol is not published; it is
//!   known only because people reverse engineered WhatsApp's own apps, and their
//!   Terms forbid exactly that. Accounts using such clients have been permanently
//!   banned with no appeal.
//!
//! Safe is the default and stays the default. Light is never selected implicitly:
//! the user has to read the warning and choose it, and the choice is stored so the
//! question is asked once rather than every launch.

use std::path::{Path, PathBuf};

const FILE: &str = "mode.txt";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The OS webview pointed at web.whatsapp.com. No account risk.
    Safe,
    /// The native protocol client. Much lighter, and can get the account banned.
    Light,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Mode::Safe => "safe",
            Mode::Light => "light",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "safe" => Some(Mode::Safe),
            "light" => Some(Mode::Light),
            _ => None,
        }
    }
}

fn path_for(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE)
}

/// The stored choice, or `None` if the user has never been asked.
pub fn load(data_dir: &Path) -> Option<Mode> {
    std::fs::read_to_string(path_for(data_dir))
        .ok()
        .as_deref()
        .and_then(Mode::parse)
}

pub fn save(data_dir: &Path, mode: Mode) {
    let _ = std::fs::write(path_for(data_dir), mode.as_str());
}

/// Forget the choice, so the next launch asks again.
pub fn clear(data_dir: &Path) {
    let _ = std::fs::remove_file(path_for(data_dir));
}

/// Resolve the mode for this launch.
///
/// `--safe` / `--light` force a mode and are remembered. `--choose` re-asks.
/// Otherwise a stored choice wins, and a first run asks.
pub fn resolve(data_dir: &Path) -> Option<Mode> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);

    if flag("--safe") {
        save(data_dir, Mode::Safe);
        return Some(Mode::Safe);
    }
    if flag("--light") {
        save(data_dir, Mode::Light);
        return Some(Mode::Light);
    }
    // The light-mode window with sample chats and no network: a way to look at the
    // UI without linking anything. Not remembered, since it is not a real choice.
    if flag("--light-demo") {
        return Some(Mode::Light);
    }
    if flag("--choose") {
        clear(data_dir);
    } else if let Some(stored) = load(data_dir) {
        return Some(stored);
    }

    let chosen = ask()?;
    save(data_dir, chosen);
    Some(chosen)
}

/// Ask the user, once. `None` means they cancelled, and the app should not start.
#[cfg(target_os = "windows")]
fn ask() -> Option<Mode> {
    use std::iter::once;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Controls::{
        TaskDialogIndirect, TASKDIALOGCONFIG, TASKDIALOG_BUTTON, TDCBF_CANCEL_BUTTON,
        TDF_ALLOW_DIALOG_CANCELLATION, TDF_USE_COMMAND_LINKS,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(once(0)).collect()
    }

    const ID_SAFE: i32 = 101;
    const ID_LIGHT: i32 = 102;

    // A command-link TaskDialog is the native Windows control for exactly this
    // shape: one question, two options, each with its own explanation.
    let safe_label = wide("Safe mode\nUses about 375 MB. Recommended.");
    let light_label = wide(
        "Light mode\nUses about 12 MB, but can get your WhatsApp account permanently banned.",
    );
    let buttons = [
        TASKDIALOG_BUTTON {
            nButtonID: ID_SAFE,
            pszButtonText: PCWSTR(safe_label.as_ptr()),
        },
        TASKDIALOG_BUTTON {
            nButtonID: ID_LIGHT,
            pszButtonText: PCWSTR(light_label.as_ptr()),
        },
    ];

    let title = wide("WhatsApp");
    let instruction = wide("How should WhatsApp run?");
    let content = wide(
        "Safe mode opens WhatsApp's real website in the browser engine already built \
         into Windows. That is exactly what using Firefox or Chrome would be, so it \
         breaks no rule and your account is never at risk.\n\n\
         Light mode talks to WhatsApp directly with no browser, which is why it is so \
         much smaller. But it does that using knowledge obtained by reverse engineering \
         WhatsApp's apps, which their Terms of Service forbid. Accounts have been \
         permanently banned for it, and there is no appeal.\n\n\
         If in doubt, choose Safe.",
    );
    let footer = wide("You can change this later by starting the app with --choose.");

    let mut config = TASKDIALOGCONFIG {
        cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
        dwFlags: TDF_USE_COMMAND_LINKS | TDF_ALLOW_DIALOG_CANCELLATION,
        dwCommonButtons: TDCBF_CANCEL_BUTTON,
        pszWindowTitle: PCWSTR(title.as_ptr()),
        pszMainInstruction: PCWSTR(instruction.as_ptr()),
        pszContent: PCWSTR(content.as_ptr()),
        cButtons: buttons.len() as u32,
        pButtons: buttons.as_ptr(),
        nDefaultButton: ID_SAFE,
        pszFooter: PCWSTR(footer.as_ptr()),
        ..Default::default()
    };

    let mut pressed = 0i32;
    let ok = unsafe { TaskDialogIndirect(&mut config, Some(&mut pressed), None, None) };
    if ok.is_err() {
        // No dialog available (very unusual). Fail safe rather than fail light.
        return Some(Mode::Safe);
    }
    match pressed {
        ID_SAFE => Some(Mode::Safe),
        ID_LIGHT => Some(Mode::Light),
        // Cancel, Escape, or the close button.
        _ => None,
    }
}

/// Non-Windows: no dialog is wired up yet, so take the safe mode rather than
/// silently opting someone into the risky one.
#[cfg(not(target_os = "windows"))]
fn ask() -> Option<Mode> {
    eprintln!(
        "No mode picker on this platform yet; defaulting to safe mode. \
         Pass --light to override."
    );
    Some(Mode::Safe)
}
