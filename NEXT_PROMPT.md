# Paste this into a fresh chat

You are picking up a WhatsApp desktop app for Michael. Everything below was built and measured
on this machine (Windows 11, 32 cores). Numbers are measured, never estimated. Read
`D:\NEWProjects\WhatsAppRs\README.md` and `DECISIONS.md` before changing direction on anything.

---

## Where it stands

**Public, released, one of everything.** `https://github.com/LunarWerxs/WhatsAppRs` (public, MIT),
v0.1.0 attached as `whatsapp-rs-0.1.0-windows-x64.zip` (164.7 MB, unzips to 347 MB). Bundled
Chromium through the `cef` crate as a child of our own window. Five processes, 490 MB logged out;
Michael measured about 893 MB on his real account, which is the mailbox, not the wrapper. Four
alternatives were built, measured and deleted; `git log` has them and FINDINGS.md has the numbers.

| logged out, whole process tree | processes | RAM | private |
| --- | --- | --- | --- |
| **this app** | 5 | 490 MB | 347 MB |
| Meta's own WhatsApp for Windows (logged IN, real account) | 8 | 1110 MB | 801 MB |
| plain Chrome, what the old C# wrapper drove | 10 | 800 MB | 571 MB |

Working, verified by running it: the page, real Windows toasts headed "WhatsApp Rs",
close-to-tray, restore, single instance, clean quit, hardware-accelerated rendering, WebRTC with
a real microphone, the exe icon and version block, and the tray menu: Open, Reload, Mute sounds,
Show notifications, Start with Windows, About, Quit. Mute is page-side and mutes only WhatsApp's
alert tones (FINDINGS.md #16 has the rule and how it was derived); `tools\mute-check.js` proves it.

## The next task: v0.2.0, ONE file (decided 2026-09-08, DECISIONS.md #23)

He looked at the release zip (17 files, 347 MB unpacked, a 0.9 MB exe beside a 271 MB
`libcef.dll`) and said it should be a single executable. It cannot be one *static* binary - CEF
only exists as that DLL, there is no static Chromium - but it can be one file to download and
double-click. Build exactly this:

1. **The exe carries the engine as a compressed payload.** Everything `bundle.ps1 -KeepFallbacks`
   produces except `whatsapp.exe` itself, compressed with LZMA/xz or zstd at a high level
   (measure both; zip gave 165 MB from 347, expect ~110 MB), embedded with `include_bytes!` or
   appended as a trailer with a footer the exe reads from its own file. Measure which starts
   faster; a 165 MB `include_bytes!` static may cost page-in time.
2. **First run unpacks once** into `%LOCALAPPDATA%\WhatsAppRs\engine\<version>\` (version from
   `CARGO_PKG_VERSION` plus a payload hash so a rebuilt 0.2.0 re-extracts). Show a small native
   "Setting up, one moment" window during the seconds it takes; never a console. Verify the
   unpack with per-file sizes or hashes before trusting it. Old engine folders from previous
   versions get deleted on a successful start of the new one.
3. **Delay-load `libcef.dll`.** Today the exe imports it at load time, so Windows refuses to
   start the exe without the DLL beside it. Add `/DELAYLOAD:libcef.dll` (and `delayimp.lib`) as
   linker args from `build.rs`, then before ANY CEF call - including the very first line of
   `main`, because CEF re-runs this exe for its renderer/GPU/utility subprocesses and those must
   find the DLL too - call `SetDllDirectoryW`/`AddDllDirectory` with the engine folder. Check
   whether `cef-dll-sys` links `libcef.lib` in a way that fights delay-load; if it does, the
   alternative is a `LoadLibraryW` + `GetProcAddress` shim, but try delay-load first.
4. **Point CEF at the folder**: `Settings.resources_dir_path`, `locales_dir_path`, and
   `browser_subprocess_path` if it helps. `chrome_elf.dll` must load before `libcef.dll` (CEF
   requirement; check the order still holds under delay-load).
5. **Prove it on a clean profile and from a directory that contains nothing else**: copy ONLY
   `whatsapp.exe` to an empty folder, run it, page loads, `tools\tray-test.ps1` 3x PASS,
   `tools\probe.ps1 -Script mute-check.js` PASS, `capability-probe.js` unchanged, then measure
   RAM with `tools\bench.ps1 -Runs 3` and show it is the same 490 MB - the payload must not stay
   mapped in memory after extraction.
6. Release as v0.2.0: the single exe as the asset, SHA-256 in the notes, README's "Download and
   run" rewritten to "download the exe, run it". Keep `bundle.ps1` as the step that produces the
   payload.

What this does NOT change, and say so in the README: disk after first run is still ~350 MB and
RAM is identical. It is the download and the one-click experience. Meta does the same thing
inside an MSIX.

## Still open, and both need his phone

1. **A real call.** WebRTC, ICE and Opus are there; **H.264 is not** (the public CEF binaries ship
   without proprietary codecs). Nobody has published what WhatsApp Web negotiates for video, so a
   real video call is the only test. If it fails, the fix is building CEF from source with
   `proprietary_codecs=true ffmpeg_branding=Chrome`: a full Chromium build, one-off, ~150 GB free.
   The same gap breaks **uploading an MP4**.
2. **Everything measured with a real account**, by the bench rather than Task Manager. The frame
   probe scrolls the chat list while measuring and there is no chat list until someone logs in.

```
tools\login.ps1                       # scan the QR; the profile persists
tools\bench.ps1 -Runs 3 -Real
tools\probe.ps1 -Script webrtc-probe.js -Real
python tools\bench-summary.py tools\bench.jsonl
```

Quit from the tray, not Task Manager: Chromium flushes cookies on a clean shutdown.

## Releasing the next version

```
# bump version in Cargo.toml, then:
tools\build.ps1
tools\tray-test.ps1                                   # 3x PASS or stop
tools\probe.ps1 -Script mute-check.js                 # PASS or stop
tools\bundle.ps1 -KeepFallbacks                       # -> D:\wa-bundle\whatsapp
Compress-Archive D:\wa-bundle\whatsapp D:\wa-bundle\whatsapp-rs-<v>-windows-x64.zip
gh release create v<v> D:\wa-bundle\whatsapp-rs-<v>-windows-x64.zip --title "v<v>" --notes-file <notes>
```

The repo is PUBLIC: before any push, say so in a heading at the top of the reply (Jacob's rule).
There is no CI yet; a workflow that builds on `windows-latest` and attaches the zip is the obvious
next thing, and the cef crate's build needs cmake, ninja and `rc.exe`, all present on that runner.

## Servo: closed, do not reopen

Servo's contributor guide bans LLM-written contributions and its `test-tidy` rejects any commit
with a `Co-Authored-By: Claude` trailer. So: the patches live on the `LunarWerxs/servo` fork as
reference (`pr/bindings-keyword-escape`, `pr/indexeddb`, `pr/web-locks`, `pr/cache-storage`, all
rebased on their `main` b03cca484 and compile-checked), a bug report was filed with the AI
disclosure at the top on Michael's instruction (DECISIONS.md #22), and nothing is offered as a pull
request. `servo-patches/` in this repo is the same material. Do not push those branches to
`servo/servo` and do not strip the trailers.

## Settled, do not re-open without being asked

- **No `--single-process`** (DECISIONS.md #17). 352 MB against 490, and he declined it: Chromium
  does not support the mode and a renderer crash takes the whole app down.
- **No whole-page audio mute** (#21). It would silence calls. Mute is the tone rule in the shim.
- **Servo, bundled Firefox, the OS webview and the native-protocol light mode are deleted**
  (#13, #14, #16). Firefox used 1115 MB and fifteen times the CPU, and cannot be embedded at all.
- **Meta's own app is not lighter** (#18): 1110 MB, and it is a WebView2 shell, so Chromium too.
- **The Android APK route leads back to light mode** (#19): 20 MB, one process, permanent ban risk.

## Traps this cost, so nobody pays for them twice

- **CEF's Chrome runtime style crashes** with a browser created as a child of a native window:
  access violation right after `create_browser` returns 1, nothing in the log. Alloy works, and
  its cost is that CEF then displays no web notifications at all.
- **The browser must be created on CEF's UI thread, and only touched there.** Anything from the
  tao thread goes through `post_ui` (a `wrap_task!` posted to `ThreadId::UI`). The `Browser` is
  parked in a static behind a `Send` wrapper for exactly that hand-off and nothing else.
- **CEF exposes no notification callback**, so the host cannot be handed a title and body. The
  bridge in `cef_view.rs` injects a page-side shim; a plain `window.Notification = Shim` does not
  stick, so it uses `Object.defineProperty` and re-installs on DOMContentLoaded.
- **The page's own tones are the only sound.** The Windows toast was always silent (notify-rust
  passes no sound name). Muting means intercepting `HTMLMediaElement.prototype.play`.
- **`Start-Process -ArgumentList @(...)` joins the array with no quoting.** A `-Run "pwsh -File x"`
  value inside it gets split and the receiving script sees `-NoProfile` as its own parameter. Pass
  one string. And `& python ...` from a detached script dies with "No process is on the other end
  of the pipe": use `Invoke-Python` and `Request-Quit` from `tools\pagescript.ps1`.
- **Never run two benches at once**; `bench.ps1` refuses. Match processes exactly, never by name
  (Meta's app is also `whatsapp.exe`) and never by a parent-child walk.
- **A deep target directory hits MAX_PATH** in cmake's compiler probe (`LNK1104 ...
  intermediate.manifest`). The repo's own `target\` is fine.

## How Michael wants you to work

- **Replies must be short.** He stopped reading long ones. Lead with the answer, plain English.
- **Measure, never assert.** Every number comes from a command you ran in that session.
- **Do it, do not recommend it**, when the action is reversible and within reach.
- He hates Chrome, Edge, Opera, Firefox as *browsers he has to run*, and Meta's official app. A
  bundled engine we ship and control is a different thing and he asked for it.
- He is, reasonably, angry that showing WhatsApp costs 500 MB. It is Meta's doing. Do not pretend
  there is a third way.
- Do not open a visible console window; detached runs go hidden with output to a log.
