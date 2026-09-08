# Test instruments

Scripts that prove the claims in README.md and FINDINGS.md by running the built app. Each one
was used for the numbers dated 2026-09-07. They expect `target\release\whatsapp.exe` to exist and
Python 3 with `websocket-client` on PATH. Run from anywhere; paths are script-relative.

## The bundled-engine head to head (2026-09-07)

The instruments for the Chromium-vs-Firefox comparison (DECISIONS.md #15). Everything here
takes `-Engine cef|firefox|webview2` and does the right per-engine thing, because the two
candidates differ in ways that break a shared script:

| script | what it proves |
| --- | --- |
| `engine-bench.ps1 -Engine X -Runs 3` | **The head-to-head.** Launches the build, waits for the PAGE to say it is ready (not a fixed sleep), samples the whole process tree's working set and private bytes at 60 s and 240 s after that, runs the frame probe, records the profile size, and quits cleanly. One JSON line per run. Refuses to start if another engine build is running. |
| `bench-summary.py [file.jsonl]` | Medians **and the min-max spread** for every configuration. Read the spread before believing a difference: on the Servo round one build measured 8.5 and 31.2 fps on consecutive runs. |
| `engine-drive.ps1 -Engine X` | The quick "does it work at all": launch, screenshot the window, print the process tree and what the page says, stop. |
| `probe-engine.ps1 -Engine X -Script Y.js` | Runs one page-side probe and prints the answer. |
| `build-cef.ps1 [-Check]` | Builds the bundled-Chromium app. Sets up cmake/ninja/MSVC and forces a SHORT target directory: cmake's compiler probe writes a path ~290 characters long under a normal temp dir and `link.exe` then fails with `LNK1104 ... intermediate.manifest`, which reads like a broken toolchain and is not. |
| `get-firefox.ps1 [-Version X]` | Downloads the Firefox the app ships and unpacks it into `runtime\firefox` **without installing it**. Mozilla publishes no Windows zip and the `.msi` is a wrapper an administrative install does not open; the NSIS `.exe` is a 7-Zip archive whose `core\` directory is the whole browser. |
| `login.ps1 -Engine X` | Opens one engine on its own persistent profile so a person can scan the QR, and leaves it running. Every measurement that matters - memory with a real mailbox, frame timing with a real chat list, a real call - needs this first. |
| `bundle.ps1 -Engine X` | Assembles the **shippable** folder (not the build directory) and reports its real size, so the disk number is what a user would install rather than what the compiler left lying around. Then run the bundle to prove the trim is honest. |
| `frames.js` | Frame timing, engine-independent, and it **scrolls the chat list while measuring**. An idle page paints nothing and every engine reports the monitor's refresh rate, which measures nothing. On a logged-out window there is no list to scroll and it says `driven: false`; treat those numbers as meaningless. |
| `page-state.js` | What the page currently is - `qr`, `syncing`, `chats` - plus row count, JS heap (Chromium only) and which browser the page thinks it is. This is what makes "60 seconds after ready" comparable between two engines that load at different speeds. |
| `webrtc-probe.js` | Whether a voice or video call can happen: RTCPeerConnection, real ICE candidate gathering, the microphone through getUserMedia, and **the codec list**. The codec list is the one that matters; see FINDINGS.md. |
| `notify-probe.js` | Fires a notification through both the page path and the service-worker path and reports what the page saw. Pair it with `toast-db.py`, which is the ground truth: on WebView2 the page reported "displayed" while Windows recorded nothing. |
| `notify-test.ps1 -Engine X [-NoBridge]` | The end-to-end toast proof for a bundled engine: checks the Start Menu shortcut, fires both notification paths, screenshots the toast corner, and prints what Windows' own database accepted. `-NoBridge` turns the host-side shim off, which is how you measure what the ENGINE does by itself rather than what our code does for it. |
| `capability-probe.js` | Everything WhatsApp needs, asked of the page: service worker registered **and controlling**, IndexedDB, SubtleCrypto, WebAssembly, SharedWorker, Web Locks, OPFS, mediaDevices, WebRTC, WebGL, notification permission. Run it after any setting that could quietly remove a capability, above all `--single-process`: a configuration that saves memory by breaking the service worker is not a saving, and none of these announce themselves. |
| `shim-check.js` | Whether the notification shim actually reached the page, and whether `Notification` is the engine's or ours. Written because the shim demonstrably ran while the constructor it was supposed to replace was still native - "it compiled" says nothing about a script injected from a different process. |
| `pagescript.ps1` | Dot-sourced helper: `Invoke-Python` and `Wait-ForExit`. Not optional plumbing. `& python` from a detached script fails intermittently with "No process is on the other end of the pipe" while querying console mode, which blanked three probes in one run and made the browser look silent; and the single-instance lock is one shared port, so a Chromium instance still shutting down makes the next Firefox launch exit 0, which reads as a crash that succeeded. |
| `gfx-probe.js` | Which renderer the engine is really using, from the page's own WebGL adapter string. Written because one engine used about five times the CPU of the other for the same page, and "it fell back to software rendering" is the first explanation worth ruling in or out rather than guessing at. |
| `chrome-control.ps1` | Plain Chrome, scratch profile, `--app`, measured by exactly the same method. It is a check on the METHOD, not a proposal: the C# wrapper measured 803 MB with this browser, so if this script gives a sane figure the engine numbers beside it can be trusted. |
| `bidi-eval.py PORT file.js` | Evaluates JS in Firefox. **Firefox removed the Chrome DevTools Protocol in 141**, so `--remote-debugging-port` speaks WebDriver BiDi now and the CDP client fails against it in a way that looks like a broken browser. |
| `cdp-eval.py PORT file.js` | The same for the Chromium builds. |

Traps these cost, so nobody rediscovers them:

- **Never run two benches at once.** The process sweep of one picks up the other's browser.
  It produced 1673 MB for a build that measures around 550, and the number looked plausible.
  `engine-bench.ps1` now refuses to start beside another engine build.
- **A cleanup that kills processes by image path reaches outside its own run.** An earlier
  `engine-drive.ps1` killed every bundled Firefox on the machine and silently destroyed a
  benchmark in another window. Kill your own tree; the job object handles the rest.
- **Both websocket clients must suppress the Origin header.** Firefox answers 400 and
  Chromium answers 403 to a DevTools/BiDi websocket that carries one, and `websocket-client`
  sends one by default. The alternative on Chromium is `--remote-allow-origins=*`, which
  loosens the browser instead of fixing the client.
- **Firefox allows one BiDi session and does not free it the moment the socket closes**, so
  back-to-back probes fail with "Maximum number of active sessions" unless the client calls
  `session.end`.

## The earlier instruments

| script | what it proves |
| --- | --- |
| `picker-drive.ps1 -Action Safe\|Light\|Cancel\|Escape\|List` | Launches `--choose`, screenshots the first-run picker, clicks the named button through UI Automation, and reports whether the process survived and what `mode.txt` says. Kills the app and clears the stored mode afterwards. |
| `measure-modes.ps1 [-Wait 30]` | Working set and private bytes of each mode's whole process tree after a settle, then restores the data dir. |
| `toast-test.ps1 [-What page\|sw\|both]` | The end-to-end toast proof: starts safe mode with a WebView2 debugging port, checks the Start Menu shortcut carries the AppUserModelID, fires a notification inside the real WhatsApp page over that port, screenshots the toast corner (`toast-<Tag>.png`), and lists what Windows' notification database accepted. |
| `shot-br.ps1 -Out file.png` | Screenshot of the bottom-right of the primary screen, where toasts draw. |
| `toast-db.py [N]` | The last N toasts Windows accepted, with the app identity each was filed under. Reads a copy of `wpndatabase.db`. This is the ground truth; the page's own "displayed" report is not. |
| `cdp-notify.py PORT page\|sw\|both` | Requests the permission and fires the notification(s) in the page; prints the page's view. |
| `cdp-eval.py PORT file.js` | Evaluates any awaited expression in the page. `survey.js` is the one that counted WhatsApp Web's notification API calls. |
| `build-servo.ps1 [-Check]` | Builds the app with safe mode on Servo (`--features servo`, `--profile servo`) in the environment Servo's `mach` would set up, and copies the ANGLE DLLs beside `target/servo/whatsapp.exe`. Slow: it compiles the engine. |
| `safe-drive.ps1 [-Exe path] [-Wait 25]` | Launches safe mode on a scratch profile and its own instance lock (so a real logged-in instance is untouched), screenshots the window, prints the process tree's memory, kills it. Works for either engine build. |
| `jank-run.ps1 -Label X [-Prefs '...']` | Restarts the real logged-in instance with a frame-timing probe, samples six seconds of animation frames thirty seconds after load, and quits cleanly so the login survives. `-Prefs` overrides engine settings for the run. **Run each configuration more than once**: variance between identical runs is larger than most differences you will be tempted to attribute to a setting. |
| `perf-test.js` | Seeds a 3,000-record store with an index and times index lookups against direct key lookups. Feed it through `WHATSAPP_RS_EVAL`. |
| `test-quit.ps1` | Proves `whatsapp.exe --quit` stops a running instance cleanly and that the cookie jar reaches disk. The cookie jar is only written on a clean shutdown, so this is the difference between keeping a WhatsApp login across a restart and having to scan the QR again. |
| `light-drive.ps1 -Mode demo\|pair [-Theme light\|dark]` | Light mode's window. `demo` opens it with sample chats and no network, moves it clear of overlays, clicks the first chat with a real mouse click (after printing what is under the cursor), types into the send box, presses Enter, screenshots before and after, and prints memory. `pair` opens the real thing as far as the QR (never scanned), screenshots it, and kills it. `-Theme` forces a palette through `WHATSAPP_RS_THEME`. |

## Environment switches the Servo build understands

Set these before launching `target/servo/whatsapp.exe`. They exist so a suspect can be tested
without a rebuild, which matters when a rebuild compiles a browser engine.

| variable | what it does |
| --- | --- |
| `WHATSAPP_RS_PROBE=1` | Every 1.5 s, ask the page which web APIs it has, what text is on screen, and what errors it caught; print it to stderr. This is how the chat-load stall was diagnosed. |
| `WHATSAPP_RS_PREFS="name=value,..."` | Override any Servo preference at startup, e.g. `dom_serviceworker_enabled=false`. No rebuild. |
| `WHATSAPP_RS_EVAL="<js>"` | Run one expression once the page loads and print the result. |
| `WHATSAPP_RS_QUIT_AFTER=<seconds>` | Shut down cleanly, the same path as the tray's Quit. Use this for persistence tests: killing the process skips the cookie flush and loses the login. |
| `WHATSAPP_RS_DATA_DIR=<path>` | Use a scratch profile, so a test instance never touches a real logged-in one. |
| `WHATSAPP_RS_INSTANCE_PORT=<port>` | Take a different single-instance lock, so a test instance can run beside a real one. |
| `--quit` (a command-line flag, not a variable) | Asks a running instance to shut down cleanly over its single-instance port. **Always stop the app this way.** Killing it skips the cookie flush and costs the WhatsApp login. |
| `RUST_LOG=warn,whatsapp_rs=info` | Engine log detail. The app's own lines are tagged `[whatsapp-rs]`, `[console …]`, `[probe …]` and `[eval]`. |

Things learned building these, so nobody rebuilds the wrong instrument:

- **The page's console is the whole game.** Until `show_console_message` was implemented in the
  embedder, every error WhatsApp logged was discarded and a hung boot looked silent.
- **Blob-URL workers do not run in Servo.** A test that creates a worker from
  `URL.createObjectURL(new Blob([...]))` fails with an empty error event, which looks exactly like
  the bug you are chasing. Verify worker-global APIs in the generated bindings instead.
- **User scripts never reach workers.** An embedder cannot polyfill a worker global from outside;
  that work has to go into the engine.

- Watching for a toast window through UI Automation does not work on this Windows build (26200):
  no `ShellExperienceHost` window ever appears; the shell lives in `ShellHost.exe`. Screenshot the
  corner and read the database instead.
- WebView2's debugging websocket refuses connections unless the runtime was started with
  `--remote-allow-origins=*` as well as `--remote-debugging-port`. Both go in
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`; the app needs no code for this.
