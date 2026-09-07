# Test instruments

Scripts that prove the claims in README.md and FINDINGS.md by running the built app. Each one
was used for the numbers dated 2026-09-07. They expect `target\release\whatsapp.exe` to exist and
Python 3 with `websocket-client` on PATH. Run from anywhere; paths are script-relative.

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
