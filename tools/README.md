# Test instruments

Every number in README.md and FINDINGS.md came from one of these. They expect a build at
`target\release\whatsapp.exe` and Python 3 with `websocket-client` on PATH. Run from anywhere;
paths are script-relative.

## Build and ship

| script | what it does |
| --- | --- |
| `build.ps1 [-Check]` | `cargo build --release` plus the cmake/ninja/MSVC environment the `cef` crate's build script needs. First run downloads the 171 MB CEF binary distribution into `%USERPROFILE%\.local\share\cef`. |
| `bundle.ps1 [-KeepFallbacks]` | Assembles the **shippable** folder - not the build directory, which holds .pdb files, headers, import libraries, 220 locales and a 20 MB CREDITS.html - and reports its real size. Then run the bundle, which is the only way to know the trim is honest rather than a list of files someone guessed were unused. |
| `single-exe.ps1 [-Codec zstd:22]` | Produces the ONE file v0.2.0 ships: runs `bundle.ps1`, compresses the sixteen engine files into a single zstd frame, and writes `whatsapp.exe` + that frame + a 64-byte footer as one executable. Windows ignores bytes past the end of a PE image, so the result is a normal exe that happens to be 129 MB. |
| `pack-payload.py <bundle> --measure` | The codec comparison behind that choice, re-runnable: size, ratio, pack time and unpack time for zstd 19/22 and xz 6/9 over the real bundle. zstd:22 won on unpack time, not on size. |
| `single-test.ps1` | The proof for the single exe, and the only one that counts: copies **only** that file into an empty folder, wipes the profile, starts it twice, and checks it starts with no `libcef.dll` beside it, unpacks 17 files, loads the page, and does **not** unpack again on the second start. |
| `login.ps1` | Opens the app on the persistent profile so a person can scan the QR, and leaves it running. Everything that matters most - memory with a real mailbox, responsiveness with a real chat list, a real call - needs this first. |

## Measure

| script | what it proves |
| --- | --- |
| `bench.ps1 -Runs 3 [-Real] [-Switches '...']` | The memory and frame-timing bench. One JSON line per run. Samples the whole process tree at 60 s and 240 s **after the page reports itself ready**, not after launch, so runs stay comparable when startup varies. Refuses to start if another build is running. |
| `bench-summary.py [file.jsonl] [--markdown]` | Medians **and the min-max spread**. Read the spread before believing a difference: on the Servo round one build measured 8.5 and 31.2 fps on consecutive runs, and a "memory win" from one sample halved the frame rate and saved nothing. |
| `chrome-control.ps1` | Plain Chrome, scratch profile, `--app`, measured by exactly the same method. A check on the METHOD, not a proposal: it returned 800.4 MB against the 802.9 MB the old C# wrapper measured a day earlier with a different script. |
| `official-app.ps1` | Meta's own WhatsApp for Windows, same method. Answers "is the real app lighter?" with a number: no - 1110 MB across 8 processes, and it is a WebView2 shell, so Chromium too. |

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
  produced 1673 MB for a build that measures 500, and the number looked entirely plausible.
- **Match processes exactly - not by name, not by a parent-child walk.** By name, because Meta's
  app and ours are both `whatsapp.exe`. By tree walk, because a re-parented process leaves an
  orphan whose parent id resolves to something that owns half the machine; that version of
  `official-app.ps1` reported `svchost` and `fontdrvhost` as part of WhatsApp.
