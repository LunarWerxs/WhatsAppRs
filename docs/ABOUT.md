# whatsapp-rs

> A native Windows tray app for WhatsApp Web that carries its own Chromium in the .exe - no Chrome, WebView2, or Meta client needed

<!-- odin:about HAND-OWNED above the GENERATED marker. Edit freely; `odin codex about --ingest` carries it back into Odin's Codex. -->

## What it is

whatsapp-rs is a small native Windows tray app that shows web.whatsapp.com inside a Chromium engine it carries itself, so the user gets a real desktop app - toasts, a tray icon, hardware-accelerated voice/video - without installing Chrome or the WebView2 runtime and without Meta's own client. It ships as a single downloadable .exe with no installer: the engine travels inside the executable as a compressed payload and unpacks itself into %LOCALAPPDATA% on first run. Written from scratch with no forked browser code; four other engine backends (the OS webview, a bundled Firefox, an embedded Servo, a native reverse-engineered protocol client) were built, measured, and deleted in favor of this one. Not affiliated with WhatsApp or Meta.

## Things not to forget

_The intricacies worth remembering: the gotchas, the half-built parts, the decisions whose
reason lives nowhere else. Odin never overwrites this section._

- ⛔ NEVER ship Chromium's `--in-process-gpu` (or `--single-process`, which implies it). It makes `viz::GpuServiceImpl::MaybeExitOnContextLost` return early, deleting Chromium's context-loss crash counter and its automatic fallback to software rendering, so a routine GPU driver reset retries forever. It did: 244 GB of log, 10.9 GB of RAM and 8.3 CPU-hours on 2026-09-09. `cargo test` guards it. anchors: `src/cef_view.rs:602`, `DECISIONS.md:306`
- The app caps its own engine log and watches its own process tree, because CEF neither rotates nor caps `cef.log` and nothing here reported the runaway above. It truncates rather than deletes: Chromium holds the log with no FILE_SHARE_DELETE, so a delete is refused while it runs. anchors: `src/watchdog.rs:1`
- The software-rendering DLLs (`vk_swiftshader.dll` and friends) are NOT optional in the bundle: they are what the out-of-process GPU falls back to, and an empty fallback stack makes Chromium stop the browser process instead. anchors: `tools/bundle.ps1:1`
- The handoff notes are no longer published: `NEXT_PROMPT.md` moved to the gitignored `docs/todo/` on 2026-09-09, and history was deliberately not rewritten because the file held nothing secret. anchors: `DECISIONS.md:371`
- Quitting cleanly (tray Quit or --quit) lets Chromium flush cookies/IndexedDB; killing the process instead can lose the WhatsApp login and force a QR re-scan. anchors: `src/cef_view.rs:351`
- Notification permission is pre-granted only for web.whatsapp.com (any other origin is denied) so Notification.permission reads granted from first paint without a real prompt. anchors: `src/cef_view.rs:820`
- Camera/mic access is auto-granted to the WhatsApp origin so a call can start; official CEF ships without H.264, so a call that won't fall back to VP8/VP9/AV1 may not connect video. anchors: `src/cef_view.rs:1012`
- Four other engine backends (OS webview/WebView2, bundled Firefox, embedded Servo, a reverse-engineered protocol client) were actually built and measured before being deleted in favor of bundled CEF. anchors: `DECISIONS.md:54`
- The exe is not code-signed, so Windows SmartScreen warns on first run - expected, not a bug to chase. anchors: `README.md:39`
- Cargo.toml deliberately avoids serde, dirs, and image crates in favor of hand-rolled parsing - do not 'simplify' by adding them back. anchors: `Cargo.toml:11`
- Stale engine versions (~350MB each) are swept only after a new version's engine has unpacked and started successfully, on a background thread, never blocking startup. anchors: `src/engine.rs:117`

<!-- odin:about GENERATED BEGIN - rewritten by `odin codex about --publish`; edit the Codex, not this -->

## What Odin knows about this project

Everything from here down is generated from this project's Codex dossier
(`codex/projects/whatsapprs.md` in the Odin clone) and is **rewritten on every publish** -
edit the dossier, not this block. Everything ABOVE the marker is yours.

### At a glance

- **Ships as:** tray app - single portable .exe (no installer, no unzip), distributed as a GitHub release asset; not code-signed
- **Written in:** Rust (12 files), PowerShell (12 files), JavaScript (8 files), Python (5 files)
- **Package:** `whatsapp-rs` 0.2.0
- **Entry points:** `cargo_bins`
- **Domain:** whatsapp, chromium, cef, system-tray, windows-toast-notifications, single-file-executable, webrtc-calls
- **Remote:** https://github.com/LunarWerxs/WhatsAppRs.git

### Architecture

- `src/` - the whole app: window creation, CEF embedding, engine unpack, tray, toast bridge, settings and geometry persistence - one Rust binary crate
- `src/cef_view.rs` - the largest module: owns the window, the embedded CEF browser, permission handling, the JS notification/mute shim injected into the page, and the tray menu's event wiring
- `src/engine.rs` - the Chromium (CEF) payload the exe carries: the single-file container format, unpack-and-verify on first run, DLL loading, and deleting stale unpacked versions
- `tools/` - every instrument that produced a number in FINDINGS.md (bench/probe scripts in PowerShell, Python and page-injected JS) plus the build, bundle, single-exe and login scripts
- `assets/` - the source .ico and pre-converted raw RGBA icon blobs embedded at compile time (no image-decoding crate)
- `servo-patches/` - dead reference patches from a deleted embedded-Servo prototype, kept only as a record
- `docs/` - README.md is the tracked docs index; docs/todo/ is the repo's single to-do file, gitignored because this repo is public
- `(root)` - Cargo.toml/build.rs (Windows manifest + icon resource), DECISIONS.md (owner rulings), FINDINGS.md/ENGINE_SURVEY.md (every measurement behind those rulings)

### Features

14 recorded - 14 shipped, 0 partial, 0 planned. Each `path:line` is where the feature is DEFINED, checked by `odin codex check`.

**Shipped**

- **WhatsApp Web in a native window** _(free)_ - Renders web.whatsapp.com inside a bundled Chromium (CEF) embedded as a child window, reporting to WhatsApp as a normal browser session. - `src/cef_view.rs:110`, `src/main.rs:39`
- **System tray with quick actions** _(free)_ - Tray icon and menu: Open, Reload, Mute sounds, Show notifications, Start with Windows, About, Quit. - `src/tray.rs:61`, `src/cef_view.rs:244`
- **Close-to-tray, single instance, remembered window position** _(free)_ - Closing the window hides it to the tray instead of quitting; a second launch brings the running instance forward instead of opening twice; window size/position is restored on the next start. - `src/cef_view.rs:274`, `src/single_instance.rs:26`, `src/geometry.rs:34`
- **Real Windows toast notifications** _(free)_ - The bundled Chromium displays no web notifications at all, so a page-side JS shim forwards the page's Notification/service-worker calls to the host, which raises a genuine Windows toast headed "WhatsApp Rs" under a registered AppUserModelID. - `src/cef_view.rs:654`, `src/cef_view.rs:388`, `src/notify.rs:35`
- **Notification permission pre-granted for WhatsApp** _(free)_ - The engine's Alloy runtime ignores permission prompts by default, so the host pre-grants the notification permission for web.whatsapp.com only (any other origin's prompt is denied) so Notification.permission reads granted from first paint. - `src/cef_view.rs:817`
- **Mute sounds toggle** _(free)_ - Silences WhatsApp's own chime and alert tones (message, sent, voice-note, call-end, ringtone) via a page-side audio-element patch; toasts still appear and calls are unaffected. - `src/cef_view.rs:294`, `src/cef_view.rs:462`
- **Voice and video calling** _(free)_ - The bundled engine has WebRTC and Opus; camera/microphone access is auto-granted to the WhatsApp origin without a prompt so a call can start instead of failing silently. No H.264, so a call that will not fall back to VP8/VP9/AV1 may not connect video. - `src/cef_view.rs:1014`
- **Start with Windows** _(free)_ - Toggling the tray item writes or removes a shortcut in the user's Startup folder (the shortcut's presence IS the setting); it launches the app minimized to the tray at logon. - `src/shortcut.rs:55`, `src/cef_view.rs:311`
- **Single-file self-extracting install** _(free)_ - The exe carries the ~350 MB Chromium engine as a compressed payload appended after the PE image; on first run it unpacks to %LOCALAPPDATA%\WhatsAppRs\engine behind a small progress window, so there is nothing to install or unzip. - `src/engine.rs:78`, `src/engine.rs:314`
- **Automatic cleanup of stale engine versions** _(free)_ - After a new version's engine unpacks and starts successfully, the previous version's ~350 MB unpacked folder is deleted in the background so upgrades do not accumulate disk usage. - `src/engine.rs:121`
- **Command-line control for scripts** _(free)_ - `whatsapp.exe --minimized` starts hidden in the tray; `whatsapp.exe --quit` asks a running instance to shut down cleanly over a loopback socket, useful from automation. - `src/cef_view.rs:130`, `src/main.rs:55`, `src/single_instance.rs:86`
- **Clean quit preserves the WhatsApp login** _(free)_ - Quitting from the tray or via --quit lets Chromium flush its cookies/IndexedDB before exit; killing the process instead can lose the login and force another QR scan. - `src/cef_view.rs:353`
- **Hardware-accelerated rendering** _(free)_ - Runs GPU-accelerated (in-process GPU, not disabled) rather than falling back to software rasterizing, which was measured to cost frame rate for a small memory saving. - `src/cef_view.rs:596`
- **No telemetry, minimal background Chromium services** _(free)_ - The app has no telemetry, no update check and no crash reporting; Chromium's own background services (sync, component updates, domain reliability, translate, media router, optimization hints, autofill server calls) are switched off on the command line. - `src/cef_view.rs:568`

### Where to add a new one

- **a new tray menu item** - add a MenuItem/CheckMenuItem and its id constant in tray.rs, append it to the Menu in tray::build, then map the id to a new UserEvent variant in cef_view.rs's MenuEvent handler and match arm. anchors: `src/tray.rs:61`, `src/cef_view.rs:244`
- **a new persisted setting/toggle** - add a field to settings::Settings, parse/serialize it in Store::load/save (key=value lines), and wire a tray toggle to flip it. anchors: `src/settings.rs:13`, `src/settings.rs:41`
- **a new CEF/Chromium command-line switch** - add to the switch list (or disable-features value) in on_before_command_line_processing; re-measure with tools/bench.ps1 before keeping it, per the file's own convention. anchors: `src/cef_view.rs:568`
- **a new platform for paths/autostart** - add a #[cfg(target_os = "...")] branch in paths.rs::platform_base and implement shortcut.rs's ensure/autostart_enabled/set_autostart for it (currently Windows-only; macOS/Linux stubs return Err/false). anchors: `src/paths.rs:23`, `src/shortcut.rs:38`
- **a new page-bridge behavior (beyond notifications/mute)** - extend NOTIFY_SHIM_JS to forward another page-side signal via console.log with a unique tag, then match that tag in the DisplayHandler console callback in cef_view.rs. anchors: `src/cef_view.rs:654`

### Gaps and wants

_Withheld: this repository is public, and the gap list is not published outside the private index._
_Read it with `python odin.py codex brief whatsapprs` in the Odin clone._

---

_Generated by `odin codex about --publish whatsapprs` on 2026-09-09 from a Codex dossier stamped 2026-09-09. Regenerate after the product moves; `odin codex about` reports drift._
<!-- odin:about GENERATED END sha=80aec0de0298 -->
