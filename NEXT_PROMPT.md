# Paste this into a fresh chat

You are picking up a WhatsApp desktop app for Michael. Everything below was built and measured
on this machine (Windows 11, 32 cores). Numbers are measured, never estimated. Read
`D:\NEWProjects\WhatsAppRs\README.md` and `DECISIONS.md` before changing direction on anything.

---

## Where it stands

**The engine question is settled and the app is down to one of everything.** Bundled Chromium,
embedded through the `cef` crate as a child of our own window. Five processes, 490 MB, 0.9 MB
binary plus 310 MB of engine. Four alternatives were built, measured and deleted; `git log` has
them and FINDINGS.md has the numbers that justified each removal.

| logged out, whole process tree | processes | RAM | private |
| --- | --- | --- | --- |
| **this app** | 5 | 490 MB | 347 MB |
| Meta's own WhatsApp for Windows (logged IN, real account) | 8 | 1110 MB | 801 MB |
| plain Chrome, what the old C# wrapper drove | 10 | 800 MB | 571 MB |

Working, verified by running it: the page, real Windows toasts headed "WhatsApp Rs",
close-to-tray, restore, single instance, clean quit, hardware-accelerated rendering, WebRTC with
a real microphone.

## The two things still open, and both need his phone

1. **A real call.** The build has WebRTC, ICE and Opus but **no H.264** - the public CEF binaries
   ship without proprietary codecs and no switch enables them. Nobody has published what
   WhatsApp Web negotiates for video, so a real video call is the only test. If it fails, the fix
   is building CEF from source with `proprietary_codecs=true ffmpeg_branding=Chrome`: a full
   Chromium build, one-off, needs ~150 GB free. The same gap also breaks **uploading an MP4**,
   which other CEF embedders have hit against WhatsApp specifically.
2. **Everything measured with a real account.** Memory here is a logged-out login page, and
   WhatsApp Web's footprint is mostly the synced mailbox. The frame probe scrolls the chat list
   while it measures and there is no chat list until someone logs in, so "how does it feel" is
   currently unanswerable.

```
tools\login.ps1                       # scan the QR; the profile persists
tools\bench.ps1 -Runs 3 -Real
tools\probe.ps1 -Script webrtc-probe.js -Real
python tools\bench-summary.py tools\bench.jsonl
```

Quit from the tray, not Task Manager: Chromium flushes cookies on a clean shutdown.

## Settled, do not re-open without being asked

- **No `--single-process`** (DECISIONS.md #17). It measures 352 MB against 490 and he declined it:
  Chromium does not support the mode and a renderer crash takes the whole app down. The switch is
  reachable for measurement via `WHATSAPP_RS_CEF_SWITCHES=single-process`, not in the build.
- **Servo, bundled Firefox, the OS webview and the native-protocol light mode are deleted**
  (#13, #14, #16). All were built and measured first. Firefox in particular used 1115 MB and
  fifteen times the CPU, and cannot be embedded at all.
- **Meta's own app is not lighter** (#18): 1110 MB, and it is a WebView2 shell, so it is Chromium
  too.
- **The Android APK route leads back to light mode** (#19). An APK is Android bytecode; the part
  worth extracting is the protocol, other people already extracted it, and that library *is* light
  mode - 20 MB, one process, permanent ban risk. It is one `git checkout` away if that risk is
  ever reconsidered.

## Traps this cost, so nobody pays for them twice

- **CEF's Chrome runtime style crashes** with a browser created as a child of a native window:
  access violation right after `create_browser` returns 1, nothing in the log. Alloy works, and
  its cost is that CEF then displays no web notifications at all.
- **The browser must be created on CEF's UI thread.** Touching a request context or creating a
  browser from the thread that called `initialize` is the same silent access violation.
- **CEF exposes no notification callback**, so the host cannot be handed a title and body. The
  bridge in `cef_view.rs` injects a page-side shim instead - and a plain
  `window.Notification = Shim` does not stick, and something on the page replaces it again after
  document start, so it uses `Object.defineProperty` and re-installs on DOMContentLoaded. It logs
  `WHATSAPP_RS_SHIM installed=...` so it cannot fail silently.
- **`& python ...` and `& app.exe --flag` from a detached PowerShell script** fail intermittently
  with "No process is on the other end of the pipe" and abort the script. Use `Invoke-Python` and
  `Request-Quit` from `tools\pagescript.ps1`.
- **Never run two benches at once.** One's process sweep picks up the other's browser: it produced
  1673 MB for a build that measures 490. `bench.ps1` now refuses.
- **Matching processes by name or by a parent-child walk both produce confident wrong numbers.**
  By name, because Meta's app and ours are both `whatsapp.exe`; by tree walk, because a
  re-parented process leaves an orphan whose parent id resolves to something that owns half the
  machine. Match exactly.
- **A build with a deep target directory hits MAX_PATH** in cmake's compiler probe and
  `link.exe` fails with `LNK1104 ... intermediate.manifest`, which reads like a broken toolchain.
  The repo's own `target\` is fine.

## How Michael wants you to work

- **Replies must be short.** He stopped reading long ones. Lead with the answer, plain English.
- **Measure, never assert.** Every number comes from a command you ran in that session.
- **Do it, do not recommend it**, when the action is reversible and within reach.
- He hates Chrome, Edge, Opera, Firefox as *browsers he has to run*, and Meta's official app. A
  bundled engine we ship and control is a different thing and he asked for it.
- He is, reasonably, angry that showing WhatsApp costs 500 MB. It is Meta's doing: their own
  Windows app is the same Chromium trick at 1110 MB, and the only lighter option is the one with
  a permanent ban risk. Do not pretend there is a third way.
- Do not open a visible console window; detached runs go hidden with output to a log.
