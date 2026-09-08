# whatsapp-rs

WhatsApp Web as a small native tray app. Windows, Linux, and macOS-shaped but untested.
Written from scratch. No forked code.

> **Current direction, 2026-09-07.** Safe mode now has four engine backends, chosen at build time,
> and two of them are new candidates being compared head to head (DECISIONS.md #15):
>
> | engine | how to build it | what the phone calls it |
> | --- | --- | --- |
> | **bundled Chromium** (`src/cef_view.rs`) | `tools/build-cef.ps1`, output `D:\ct\cefapp` | Chrome |
> | **bundled Firefox** (`src/firefox_view.rs`) | `cargo build --release --features firefox` | Firefox |
> | the OS webview (`src/webview.rs`) | `cargo build --release` | Microsoft Edge (rejected) |
> | Servo (`src/servo_view.rs`) | retired, DECISIONS.md #13 | Firefox |
>
> `WHATSAPP_RS_ENGINE=cef|firefox|webview2` picks one at run time within a build that contains it.
> The comparison, and the two findings that decide it, are the last section of FINDINGS.md.
> Light mode is retired (DECISIONS.md #14); its code is still here and unmaintained.

**786 KB release binary.** The rendering engine is the one your OS already ships, so nothing is
bundled: WebView2 on Windows, WebKitGTK on Linux, WKWebView on macOS.

## The two bundled engines, measured 2026-09-07

Logged out, same instruments, whole process tree, sampled after the page reported itself ready.
Full method and every configuration in FINDINGS.md; the short version:

| | processes | RAM (working set) | RAM (private) | CPU to load | engine on disk | phone shows |
| --- | --- | --- | --- | --- | --- | --- |
| **bundled Chromium, one process** | **1** | **352 MB** | 294 MB | **6 s** | 325 MB | Chrome |
| bundled Chromium, default | 7 | 548 MB | 385 MB | 7 s | 325 MB | Chrome |
| **bundled Firefox, best of six configs** | 10 | **1112 MB** | 1030 MB | 41 s | 344 MB | Firefox |
| *(control)* the OS webview, as shipped today | 3 | 376 MB | 201 MB | 12 s | nothing | Microsoft Edge |
| *(control)* plain Chrome, what the C# app drives | 10 | 800 MB | 571 MB | 12 s | already installed | Chrome |

The last row is the check on the method, not a proposal: it reproduces the 803 MB the C# wrapper
measured on a different day with a different script, so the numbers beside it can be trusted.

**Chromium wins on everything measurable and loses on one thing that is not.** The official CEF
binaries carry no H.264, and there is no switch for it - so WhatsApp video calls that insist on
H.264 and, already documented by other CEF users, **uploading an MP4 to WhatsApp**, are the price.
Firefox has H.264 and also raises real Windows toasts by itself, where Chromium needs the host to
draw them. See FINDINGS.md, "Bundled Chromium vs bundled Firefox".

## Measured against the C# Chrome wrapper it replaces (the OS-webview build)

| | This app | Old C# wrapper |
| --- | --- | --- |
| Binary | **786 KB** | 96 KB |
| RAM, idle on the login page | **375 MB** | 803 MB |
| Processes | **3** (1 ours + 2 engine) | 10 |
| Data directory, fresh | **31 MB** | 103 MB |
| Data directory, projected with full history | **about 65 MB** | 343 MB |
| Requires Chrome installed | no | yes |

Those RAM and process numbers are after tuning. Stock configuration was 633 MB across 8 processes;
`--single-process` plus a set of feature-disabling switches took it to 375 MB across 3, verified not
to break the service worker or notifications. Set `WHATSAPP_RS_MULTIPROCESS=1` to fall back to the
stock multi-process engine if single-process ever misbehaves.

**The remaining 352 MB is WhatsApp's own application, not this wrapper.** Measured: its JavaScript
heap alone is 68.7 MB used / 97.1 MB allocated, from 20 scripts totalling 25.3 MB decoded, on a
logged-out login screen with 382 DOM nodes. No engine choice avoids that. Our Rust process is 23 MB
of the total.

The profile shrinks because a WebView2 profile creates none of Chrome's browser-only baggage.
Verified absent: Safe Browsing lists, optimization_guide_model_store, component_crx_cache,
WasmTtsEngine, OnDeviceHeadSuggestModel, ActorSafetyLists.

Honest framing: the engine is Chromium on Windows either way. The saving is our binary and our
profile, not the renderer. See FINDINGS.md.

## What works, verified by running it

- **Loads WhatsApp Web** to the QR login screen, on Windows and on Linux/WebKitGTK 2.50.6. No
  user-agent spoof needed on either.
- **Notifications, as real Windows toasts.** Both engines deny the permission by default and each
  needs a different host-side fix (`src/notify.rs`). On Windows that is not enough: WebView2 never
  hands a web notification to Windows on its own (measured: the page's `onshow` fired, Windows'
  notification database recorded nothing). The app now handles `NotificationReceived` itself and
  raises the toast, and writes the Start Menu shortcut carrying the AppUserModelID that Windows
  requires (`src/shortcut.rs`). Verified 2026-09-07 by screenshot: a toast from this app, on screen.
  See `tools/README.md` for the instrument that proves it.
- **Light mode has a chat window in WhatsApp's own shape** (`src/chat_ui.rs` and the `ui_*.rs`
  modules): WhatsApp Web's layout and palette, light and dark following the Windows setting,
  drawn by the app with GDI and GDI+. Pairing QR in the window, avatars, chat rows with preview,
  time and unread badge, search, chat header, date pills, bubbles, composer with Enter to send,
  history and contact names imported from the phone, messages kept between runs, close-to-tray.
  Verified 2026-09-07 with `tools/light-drive.ps1` in both themes: a click on a chat, a typed
  message, and the reply, at a 20 MB working set. `whatsapp.exe --light-demo` shows it with
  sample chats and no network; `WHATSAPP_RS_THEME=light|dark` forces a theme.
- **Close to tray.** Verified: the window hides and the process survives, for both the X button and
  Alt+F4.
- **Single instance.** Verified: a second launch exits on its own and raises the first.
- **Window geometry persists**, and the file is only written when it actually changes. Verified: six
  ticks with an unmoved window produced zero writes.

## What does not work

- **macOS is unbuilt and untested.** wry's WKWebView backend does not implement the notifications
  permission at all (see wry's own `src/permissions.rs`), so Mac needs a hand-written native bridge.
  Everything else should port. No Mac was available.
- **No voice or video calls on Linux.** Most distributions build WebKitGTK without WebRTC. Measured
  here (`hasWebRTC: false`) and independently confirmed by other projects. Linux is
  messaging-complete, calls-absent, by decision.
- **No OPFS on Linux.** Harmless today because WhatsApp stores in IndexedDB, but a future-breakage risk.
- **No tray icon on stock GNOME Wayland** without a user-installed extension. GNOME removed legacy
  tray support.
- **Service-worker notifications on Windows.** WebView2 raises `NotificationReceived` for the
  page's `new Notification()` and never for a service worker's `showNotification()`; measured
  under both the single-process flags and the multi-process fallback, no toast and no database row
  either way. WhatsApp Web's loaded bundle uses `new Notification(` (that path is bridged) and has a
  single `showNotification` reference, so a background alert sent that way would be lost.

## Layout

| file | role |
| --- | --- |
| `src/main.rs` | entry; resolves the mode, dispatches, shows fatal errors in a message box |
| `src/mode.rs` | Safe/Light, the stored choice, and the first-run picker |
| `src/webview.rs` | safe mode: window, event loop, close-to-tray, wiring |
| `src/light.rs` | light mode: the protocol client on its own thread, events up and sends down; `--light-demo` |
| `src/chat_ui.rs` | light mode window: main window, headers, composer, screens, events |
| `src/ui_theme.rs` | WhatsApp's palette (light and dark), fonts, GDI+ shape helpers |
| `src/ui_chatlist.rs` | the chat list panel, drawn row by row |
| `src/ui_messages.rs` | the conversation panel: date pills and bubbles, cached layout |
| `src/chats.rs` | light mode store: chats and messages, persisted as `light-chats.json` |
| `src/notify.rs` | the notification permission fixes, per engine, the JS shim, and the Windows toast bridge |
| `src/shortcut.rs` | the Start Menu shortcut carrying the AppUserModelID, written only when stale |
| `src/tray.rs` | tray icon and menu |
| `build.rs` | embeds the Windows manifest; without it the exe dies at load (Common Controls v6) |
| `src/geometry.rs` | window placement, written only on change |
| `src/single_instance.rs` | loopback-port lock, portable, no dependencies |
| `src/paths.rs` | per-OS data directory |
| `src/bin/probe.rs` | the capability probe that established all of the above |

Three classes from the C# original do not exist here, because the app owns its own window:
`ChromeFinder` (no external browser to locate), `WindowFinder` (no foreign window to hunt), and
`Hooks` (no global mouse and keyboard hook; close-to-tray is one match arm).

## Build

```
cargo build --release
```

That is safe mode on the OS webview. Safe mode on Servo (DECISIONS.md #8 and #12: our own engine,
the same on every OS, presenting as Firefox) is the `servo` feature, built against the sibling
checkout at `../servo` on its `cache-storage-complete` branch:

```
pwsh -File tools/build-servo.ps1
```

It reproduces the environment Servo's `mach` sets up on Windows (SERVO_BUILD.md), uses the
`servo` profile (no LTO; an engine's worth of code), and copies the ANGLE DLLs beside
`target/servo/whatsapp.exe`.

Linux build dependencies: `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev
librsvg2-dev libsoup-3.0-dev pkg-config`.

## The probe

`cargo run --bin probe`, then read `probe-report.jsonl`. It reports which web APIs the local engine
has and whether notifications actually fire. `Dockerfile.linux-probe` runs the same probe against
WebKitGTK headlessly.

## Icons

`assets/icon_32.rgba` and `icon_256.rgba` are raw RGBA, pre-converted so there is no image-decoding
dependency. To regenerate after changing `assets/icon.ico`:

```python
from PIL import Image
for size in (32, 256):
    frame = Image.open("assets/icon.ico").convert("RGBA").resize((size, size), Image.LANCZOS)
    with open(f"assets/icon_{size}.rgba", "wb") as f:
        f.write(frame.tobytes())
```
