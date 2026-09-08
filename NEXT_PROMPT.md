# Paste this into a fresh chat

You are picking up a WhatsApp desktop app for Michael. Everything below was built and measured
on this machine (Windows 11, 32 cores). Numbers are measured, never estimated. Read
`D:\NEWProjects\WhatsAppRs\FINDINGS.md` (last section) and `DECISIONS.md` before changing
direction on anything.

---

## Where it stands

**The Chromium-vs-Firefox comparison DECISIONS.md #15 asked for is done.** Both builds exist,
both work, both were measured. The answer is Chromium, and the only thing arguing the other way
is one missing codec. What remains is Michael's decision and two measurements that need his
phone.

| logged out, whole process tree | processes | working set | private | CPU to load | engine on disk |
| --- | --- | --- | --- | --- | --- |
| **bundled Chromium, `--single-process`** | **1** | **352 MB** | 294 MB | **6 s** | 325 MB |
| bundled Chromium, default | 7 | 548 MB | 385 MB | 7 s | 325 MB |
| bundled Firefox, best of six configurations | 10 | 1112 MB | 1030 MB | 41 s | 344 MB |
| *control:* the OS webview (Edge's engine) | 3 | 376 MB | 201 MB | 12 s | nothing |
| *control:* plain Chrome, what the C# app drives | 10 | 800 MB | 571 MB | 12 s | installed |

The last row is the check on the method: it reproduces the 802.9 MB the C# wrapper measured on
another day with another script, so the rest can be trusted.

## What each build is

- **`src/cef_view.rs`** - Chromium embedded, through the `cef` crate (CEF 152). A real child
  window inside ours, so tray, close-to-tray, single instance and geometry are unchanged.
  Build: `tools\build-cef.ps1` -> `D:\ct\cefapp\whatsapp.exe`. The page reports **Chrome 152**;
  what the phone's linked-device list says is not confirmed until someone links it.
- **`src/firefox_view.rs`** - a real Firefox 155.0.1 we ship, launched on a private profile and
  its window adopted with `SetParent`. Build: `cargo build --release --features firefox`.
  Get the runtime with `tools\get-firefox.ps1`. The page reports **Firefox 155**; same caveat.
- `src/webview.rs` (OS webview) and `src/servo_view.rs` (retired) still build.
  `WHATSAPP_RS_ENGINE=cef|firefox|webview2` picks one inside a build that has it.

## The two things only Michael can settle

1. **The codec.** The public CEF binaries have **no H.264** and no switch turns it on. Firefox
   has it. That costs a WhatsApp video call that will not fall back to VP8, and it costs
   uploading an MP4 (other CEF users have hit exactly that against WhatsApp). Nobody has
   published what WhatsApp Web actually negotiates, so **a real call from the Chromium build is
   the test**. If it matters, the fix is building CEF with
   `proprietary_codecs=true ffmpeg_branding=Chrome`: a full Chromium build, a one-off job on a
   machine with ~150 GB free, not a rebuild here.
2. **Single process or not.** `--single-process` is what buys 352 MB instead of 548. Chromium
   does not officially support it and a renderer crash takes the whole app down instead of
   showing a sad tab. Nothing measurable breaks (`tools\capability-probe.js`: service worker
   registered and controlling, IndexedDB, Web Locks, OPFS, WebRTC, microphone, all present).
   The WebView2 build already made this trade. It is his risk to accept.

## The measurements that need his phone, and how to take them

Everything above is a logged-out login page. Three things cannot be measured that way:

```
tools\login.ps1 -Engine cef        # scan the QR; each engine keeps its own profile
tools\login.ps1 -Engine firefox
tools\engine-bench.ps1 -Engine cef     -Runs 3 -Real
tools\engine-bench.ps1 -Engine firefox -Runs 3 -Real
python tools\bench-summary.py tools\bench-engines.jsonl
```

- **Memory with a real mailbox.** WhatsApp Web's footprint is mostly the synced account.
- **Responsiveness.** `tools\frames.js` scrolls the chat list while it measures, and there is
  no chat list until someone logs in. Logged out it reports the monitor's refresh rate for
  both engines, which means nothing.
- **A real voice and video call.** Both engines gather ICE candidates and open the microphone.
  Whether a call connects, and on which codec, is the open question above.

Neither login touches `%LOCALAPPDATA%\WhatsAppRs`, which belongs to the Servo build and holds
his live session. Quit from the tray, not Task Manager: both engines flush on a clean shutdown.

## What is dead. Do not revive either without being asked

- **Servo**, retired 2026-09-07 (DECISIONS.md #13): ~30 fps, 300 ms stalls, ~1.2 GB, no WebRTC
  ever. His verdict: "Servo has been, like, shitty."
- **Light mode**, retired 2026-09-07 (#14): the native-protocol client, ~20 MB and a permanent
  ban risk. "I don't really care about that."

Both still compile and all their measurements and the nine upstreamable Servo patches are still
in the tree. Deleting them is a one-line ask.

## Traps this round cost, so nobody pays for them twice

- **CEF's Chrome runtime style crashes** with a browser created as a child of a native window:
  access violation right after `create_browser` returns 1, nothing in the log. Alloy style
  works, and its cost is that CEF then displays no web notifications at all.
- **The browser must be created on CEF's UI thread.** Touching a request context or creating a
  browser from the thread that called `initialize` is the same silent access violation.
- **`cargo build` for CEF needs a SHORT target directory.** cmake's compiler probe writes a
  ~290-character path and `link.exe` fails with `LNK1104 ... intermediate.manifest`, which
  reads like a broken toolchain. `tools\build-cef.ps1` forces `D:\ct`.
- **Firefox dropped the Chrome DevTools Protocol in 141.** `--remote-debugging-port` speaks
  WebDriver BiDi now; `tools\bidi-eval.py` is the client, `tools\cdp-eval.py` is Chromium's.
  Both must suppress the `Origin` header or the handshake is refused (400 / 403).
- **`Path::canonicalize` returns a `\\?\` path and `QueryFullProcessImageNameW` does not**, so
  comparing them never matches. That made the Firefox window search silently fail, and the job
  object then killed the working browser.
- **`& python` from a detached PowerShell script fails intermittently** with "No process is on
  the other end of the pipe". It blanked three probes in one run. `tools\pagescript.ps1` is the
  fix; use `Invoke-Python`.
- **Never run two benches at once.** One's process sweep picks up the other's browser: it
  produced 1673 MB for a build that measures 550, and the number looked plausible.
  `engine-bench.ps1` now refuses.
- **A page-side shim needs `Object.defineProperty` and a re-check.** Plain assignment to
  `window.Notification` did not stick, and something on the page replaces it again after
  document start. The shim logs `WHATSAPP_RS_SHIM installed=...` so this cannot fail silently.

## How Michael wants you to work

- **Replies must be short.** He stopped reading long ones. Lead with the answer, plain English.
- **Measure, never assert.** Every number comes from a command you ran in that session.
- **Do it, do not recommend it**, when the action is reversible and within reach.
- He hates Chrome, Edge, Opera, Firefox as *browsers he has to run*, and Meta's official app.
  A bundled engine we ship and control is a different thing and he asked for it.
- Do not open a visible console window; detached runs go hidden with output to a log.
