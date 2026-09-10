# Test instruments

Every number in README.md and FINDINGS.md came from one of these. They expect a build at
`target\release\whatsapp.exe` and Python 3 with `websocket-client` on PATH. Run from anywhere;
paths are script-relative.

## Build and ship

| script | what it does |
| --- | --- |
| `build.ps1 [-Check] [-Test]` | `cargo build --release` plus the cmake/ninja/MSVC environment the `cef` crate's build script needs. First run downloads the 171 MB CEF binary distribution into `%USERPROFILE%\.local\share\cef`. **`-Test` runs the unit tests**, which need no CEF, no GPU and no network and finish in milliseconds: the log-cap and onset-snapshot logic in `watchdog.rs`, and `in_process_gpu_is_never_shipped`, which fails if `--in-process-gpu` or `--single-process` gets back into the shipped switch list. That one is the guard the 2026-09-09 incident bought. |
| `bundle.ps1` | Assembles the **shippable** folder - not the build directory, which holds .pdb files, headers, import libraries, 220 locales and a 20 MB CREDITS.html - and reports its real size. Then run the bundle, which is the only way to know the trim is honest rather than a list of files someone guessed were unused. The `-KeepFallbacks` switch is **gone** as of 2026-09-09: the software-rendering DLLs are now always included, because they are what Chromium falls back to after a GPU context loss, and this script's default output folder is the one a live install runs from. |
| `single-exe.ps1 [-Codec zstd:22]` | Produces the ONE file this repo ships: **runs the unit tests**, then `bundle.ps1`, compresses the sixteen engine files into a single zstd frame, and writes `whatsapp.exe` + that frame + a 64-byte footer as one executable. Windows ignores bytes past the end of a PE image, so the result is a normal exe that happens to be 129 MB. The tests gate the release because this repository has no CI, and a guard nobody runs is not a guard; `-SkipTests` exists but wanting it is a smell. |
| `pack-payload.py <bundle> --measure` | The codec comparison behind that choice, re-runnable: size, ratio, pack time and unpack time for zstd 19/22 and xz 6/9 over the real bundle. zstd:22 won on unpack time, not on size. |
| `single-test.ps1` | The proof for the single exe, and the only one that counts: copies **only** that file into an empty folder, wipes the profile, starts it twice, and checks it starts with no `libcef.dll` beside it, unpacks 17 files, loads the page, and does **not** unpack again on the second start. |
| `whatsapp-portable.cmd` | **Not a test instrument, a shipped convenience.** Put it beside the released exe and it runs the app entirely out of that one folder - engine, profile, settings and log - by setting `WHATSAPP_RS_ENGINE_DIR` and `WHATSAPP_RS_DATA_DIR` to subfolders of itself. It also sets a different `WHATSAPP_RS_INSTANCE_PORT`, because the single-instance lock is a loopback port and without that a portable copy started beside an installed one would silently raise the installed one's window and exit. The Start Menu shortcut is still written: Windows will not show a toast without the AppUserModelID it carries. |
| `login.ps1` | Opens the app on the persistent profile so a person can scan the QR, and leaves it running. Everything that matters most - memory with a real mailbox, responsiveness with a real chat list, a real call - needs this first. |

## Measure

| script | what it proves |
| --- | --- |
| `bench.ps1 -Runs 3 [-Real] [-Switches '...']` | The memory and frame-timing bench. One JSON line per run. Samples the whole process tree at 60 s and 240 s **after the page reports itself ready**, not after launch, so runs stay comparable when startup varies. Refuses to start if another build is running. |
| `bench-summary.py [file.jsonl] [--markdown]` | Medians **and the min-max spread**. Read the spread before believing a difference: on the Servo round one build measured 8.5 and 31.2 fps on consecutive runs, and a "memory win" from one sample halved the frame rate and saved nothing. |
| `chrome-control.ps1` | Plain Chrome, scratch profile, `--app`, measured by exactly the same method. A check on the METHOD, not a proposal: it returned 800.4 MB against the 802.9 MB the old C# wrapper measured a day earlier with a different script. |
| `official-app.ps1` | Meta's own WhatsApp for Windows, same method. Answers "is the real app lighter?" with a number: no - 1110 MB across 8 processes, and it is a WebView2 shell, so Chromium too. |

## Survive a fault

There was no section here until 2026-09-09, and that is why a GPU driver reset turned the app
into a 244 GB log file, 10.9 GB of RAM and 8.3 CPU-hours of one spinning thread before anybody
noticed. Every other instrument above runs for four minutes against a login page; nothing ran
long, and nothing broke anything on purpose.

| script | what it proves |
| --- | --- |
| `soak.ps1 [-Minutes N]` | Fifteen checks over the runaway (twelve with `-Minutes 0`). The first is the root cause: **a `--type=gpu-process` child must exist**, because `--in-process-gpu` is what removed Chromium's context-loss crash counter and its automatic fallback to software rendering. The rest exercise `watchdog.rs` against the real running engine - that Chromium is holding `cef.log` open, that deleting it is refused while it is (which is why the watchdog truncates instead), that passing the cap rolls it back to empty under that open handle, that the onset snapshot survives, that a sustained firehose raises the alarm, that private bytes plateau over the soak, and that `--quit` still works afterwards. `-Minutes 0` runs the structural half in about ninety seconds. |

To exercise the real path rather than the simulated one, start `soak.ps1 -Minutes 30` and press
**Ctrl+Shift+Win+B** once it is up. That restarts the display driver and delivers a genuine
`DXGI_ERROR_DEVICE_REMOVED` to the GPU process - the exact fault of 2026-09-09. Everything on
screen blacks out for a second or two. The app should still be there afterwards.

## Probe the page

`probe.ps1 -Script X.js` launches the app with a debugging port, evaluates the script in the
page, prints the answer and stops the app. `-Real` runs it against the logged-in profile.

| script | what it proves |
| --- | --- |
| `capability-probe.js` | Everything WhatsApp needs, asked of the page: service worker registered **and controlling**, IndexedDB, SubtleCrypto, WebAssembly, SharedWorker, Web Locks, OPFS, mediaDevices, WebRTC, WebGL, notification permission. Run it after any switch that could quietly remove a capability. A configuration that saves memory by breaking the service worker is not a saving, and none of these announce themselves. |
| `webrtc-probe.js` | Whether a call can happen: RTCPeerConnection, a real ICE gathering pass, the microphone through getUserMedia, and **the codec list**. The codec list is the one that matters - it is how the missing H.264 was found. |
| `gfx-probe.js` | Which renderer is really in use, from the page's WebGL adapter string. Written because a memory setting can silently drop Chromium onto a software rasteriser, which is the shape of the Servo round's mistake. |
| `page-state.js` | What the page currently is - `qr`, `syncing`, `chats` - plus row count, JS heap and which browser it thinks it is. This is what makes "60 seconds after ready" mean the same thing across runs. |
| `frames.js` | Frame timing, and it **scrolls the chat list while measuring**. An idle page paints nothing and reports the monitor's refresh rate, which measures nothing. Logged out it says `driven: false`; treat those numbers as meaningless. |
| `shim-check.js` | Whether the notification shim reached the page, and whether `Notification` is the engine's or ours. Written because the shim demonstrably ran while the constructor it was supposed to replace was still native. |
| `mute-check.js` | Whether the tray's "Mute sounds" reaches the page and mutes **only** the alert tones: a detached element on an https source is muted while the toggle is on, an element in the document and a blob-backed one are never touched. Seed the probe profile's `settings.txt` with `mute_sounds=1` first and it also proves the host pushed the state at load. |
| `script-grep.py PORT REGEX` | Grep the JavaScript the page actually loaded, through the debugger. The bundles are cross-origin so the page cannot read them, but `Debugger.getScriptSource` can. This is how the mute rule was derived: every WhatsApp alert is a module-level `new window.Audio(<static asset>)`, voice messages play from blobs, calls from a MediaStream. |

## Notifications and product behaviour

| script | what it proves |
| --- | --- |
| `notify-test.ps1 [-NoBridge]` | The end-to-end toast proof: checks the Start Menu shortcut, fires both notification paths, screenshots the toast corner, and prints what **Windows' own database** accepted. `-NoBridge` turns the host shim off, which is how you measure what the engine does alone: nothing at all. |
| `notify-probe.js` / `toast-db.py` | The two halves of that. The page's report is worth nothing on its own - it said "displayed" for every notification while Windows recorded none - so `toast-db.py` is the ground truth. |
| `tray-test.ps1` | Close-to-tray, restore from a second launch, and clean quit. Writing this test is what found that `--quit` was being silently ignored, which meant every restart ended in a kill, which skips the cookie flush and loses the WhatsApp login. |

## Plumbing

`pagescript.ps1` is dot-sourced by the rest and holds `Invoke-Python`, `Request-Quit` and
`Wait-ForExit`. It is not optional convenience:

- **`& python ...` and `& app.exe --flag` fail intermittently from a detached script** with the
  Win32 error "No process is on the other end of the pipe" while querying console mode. Under
  `ErrorActionPreference = Stop` that aborts a whole benchmark configuration; without it, the
  call silently returns nothing. It corrupted two runs and blanked three probes before it was
  understood. `Start-Process -WindowStyle Hidden` with redirected files never touches a console.
- **Piping one of these scripts into `Select-Object -First N` kills it.** PowerShell throws
  StopUpstreamCommandsException at the producer as soon as N objects have arrived, so everything
  after the line that produced the Nth object never runs, including the cleanup that stops the
  app. Five orphaned processes then held `whatsapp.exe` open and the next `cargo build` failed
  with "Access is denied". Redirect to a file, or use `-Last N`, which has to read to the end.
- **The single-instance lock is one TCP port**, so an instance still shutting down makes the next
  launch exit 0 - which reads as a crash that succeeded. `Wait-ForExit` is why that stopped
  happening.

`cdp-eval.py` is the DevTools client. It must suppress the `Origin` header or Chromium answers
403; the alternative is `--remote-allow-origins=*`, which loosens the browser instead of fixing
the client.

## Two rules for anyone adding to this

- **Never run two measurements at once.** One's process sweep picks up the other's browser. It
  produced 1673 MB for a build that measures 500, and the number looked entirely plausible. It
  also leaves the loser's processes holding `whatsapp.exe`, and the next `cargo build` then fails
  with "Access is denied" - which reads as a broken toolchain and is not. `bench.ps1` and
  `soak.ps1` both refuse to start when a `whatsapp.exe` from a different path is up.
- **A script must never force-kill a `whatsapp.exe` it did not start.** `Wait-ForExit` does
  exactly that after 40 seconds, and the owner's real, logged-in instance is a `whatsapp.exe`
  too. A kill skips Chromium's cookie flush, and the phone has to re-pair. Refuse and say whose
  process it is; do not clear the way.
- **Match processes exactly - not by name, not by a parent-child walk.** By name, because Meta's
  app and ours are both `whatsapp.exe`. By tree walk, because a re-parented process leaves an
  orphan whose parent id resolves to something that owns half the machine; that version of
  `official-app.ps1` reported `svchost` and `fontdrvhost` as part of WhatsApp.
