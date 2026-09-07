# WhatsApp desktop app: handoff

Paste this whole file to the next AI. Everything below was built and measured on this
machine (Windows 11, 32 cores). Numbers are measured, never estimated.

---

## What this is

Michael (the owner) wanted a small WhatsApp desktop app to replace a C# wrapper that
shells out to Google Chrome. It grew into three working builds. The current app has
**two modes behind a first-run picker**, and it compiles clean today.

Repos, all on this machine, none pushed anywhere:

| path | what it is | state |
| --- | --- | --- |
| `D:\NEWProjects\WhatsAppRs` | **the main app** (both modes) | builds clean, `cargo build --release --bin whatsapp`; picker and toasts verified on screen 2026-09-07 |
| `D:\NEWProjects\WhatsAppNative` | standalone native-protocol probe | builds, measured at 12 MB |
| `D:\NEWProjects\WhatsAppWebApp` | decompiled C# original, for reference | builds |
| `D:\NEWProjects\servo` | Servo fork with my Cache Storage work | builds, 4 commits on branch `cache-storage-complete` |

## The measured numbers (this is the whole story)

| approach | RAM | processes | disk | account ban risk |
| --- | --- | --- | --- | --- |
| old C# Chrome wrapper (what he runs today) | 803 MB | 10 | 343 MB | none |
| **safe mode**, OS webview at web.whatsapp.com | **375 MB** | 3 | 31 MB | **none** |
| Servo engine (not Chromium) | 464 MB | 1 | ~30 MB | none |
| **light mode**, native protocol, no browser | **12 MB** | **1** | 4 KB | **real, permanent** |

WhatsApp Web's own JavaScript heap is 68.7 MB used / 97.1 MB allocated on a logged-out
screen. That is why no browser-based option gets small: the floor is their app, not the
engine.

## ⚠ The ban rule, and Michael's standing decision

- **Loading web.whatsapp.com in any browser engine is completely safe.** WhatsApp's Terms
  say nothing about which browser you use. Chrome, Firefox, Edge, Opera and Safari are all
  on their own supported list. Safe mode is identical in kind to using Firefox.
- **The native protocol client is what carries risk.** The protocol is unpublished; it is
  known only from reverse-engineering WhatsApp's apps, which their Terms forbid verbatim:
  *"reverse engineer, alter, modify, create derivative works from, decompile, or extract
  code from our Services."* Accounts have been permanently banned, with no appeal.
- **Michael's decision, stated plainly: he will never use anything with ban risk on his own
  number, and he refuses to get a spare number.** He wants light mode to *exist* as an
  option for other people, but he personally runs safe mode. Do not push back on this and
  do not quietly make light mode the default.

## The app as it stands

`D:\NEWProjects\WhatsAppRs`, one binary named `whatsapp`, ~1,150 lines of our own code.

```
src/main.rs             entry; resolves the mode, dispatches, shows fatal errors in a message box
src/mode.rs             Mode enum, the stored choice, and the first-run picker
src/webview.rs          SAFE MODE: tao window + wry webview + tray + close-to-tray
src/light.rs            LIGHT MODE: whatsapp-rust client on its own thread; events up to the
                        window through a Sink, sends down through a channel; --light-demo
src/chat_ui.rs          LIGHT MODE window: main window, headers, composer, screens, events
src/ui_theme.rs         WhatsApp's palette (light/dark), fonts, GDI+ shape helpers
src/ui_chatlist.rs      the chat list panel, drawn row by row
src/ui_messages.rs      the conversation panel: date pills and bubbles, cached layout
src/chats.rs            LIGHT MODE store: chats and messages, persisted as light-chats.json
src/notify.rs           notification permission grants per engine, toast(), and the Windows
                        bridge that turns the page's notifications into real toasts
src/shortcut.rs         Start Menu shortcut carrying the AppUserModelID, written only when stale
src/tray.rs             tray icon and menu (embedded raw RGBA icons, no image crate)
build.rs                embeds the Windows manifest (Common Controls v6); without it the exe
                        dies at load before main runs
src/geometry.rs         window position, written only when it changes
src/paths.rs            per-OS data directory
src/single_instance.rs  loopback-port lock, portable
src/bin/probe.rs        capability probe that established what each engine supports
src/bin/bench.rs        memory sweep harness
```

**The picker**: first launch shows a native Windows TaskDialog with two command links:
"Safe mode, about 375 MB, recommended" and "Light mode, about 12 MB, but can get your
WhatsApp account permanently banned". Cancel exits without starting. The answer is stored
in `mode.txt` in the data dir. `--choose` re-asks; `--safe` / `--light` force it.

**Safe mode is fully working**: window, tray icon, close-to-tray (X and Alt+F4), single
instance, window geometry, and notifications. All verified by running it.

**Light mode has a working chat window as of 2026-09-07** (`src/chat_ui.rs`, `src/chats.rs`):
QR drawn in the window, chat list, message view, send box, history and names imported,
messages persisted between runs, close-to-tray. It works and it is ugly; see step 3 below
and DECISIONS.md #11. `--light-demo` shows it with sample chats and no network.

## What is verified vs not

Verified by actually running it:
- Safe mode renders WhatsApp Web, tray and close-to-tray work, single instance works,
  geometry writes only on change (6 idle ticks produced 0 writes).
- Notifications: both engines *deny* them by default. Fixed on Windows via
  `add_PermissionRequested` granting `COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS`, and on
  Linux via `WebContext::initialize_notification_permissions` (the `permission-request`
  signal never fires there, so that is a dead end; do not retry it).
- Light mode reached WhatsApp's servers and produced a live pairing code at 12 MB.

Verified 2026-09-07 (session two), all by running it, instruments in `tools/`:
- **The exe as handed over never started.** Exit code -1073741511 (STATUS_ENTRYPOINT_NOT_FOUND)
  in 1.4 s, no window. `TaskDialogIndirect` needs Common Controls v6 and the exe had no
  manifest. `build.rs` embeds one now. The picker then showed; Safe, Light, Cancel and Escape
  were each driven by UI Automation and did the right thing (`mode.txt` written only for the
  two real choices, process exits on Cancel/Escape).
- **A real Windows toast from this app has now been seen on screen** (screenshot, title
  "WhatsApp Rs", the page's text). It took three fixes, each measured: (1) the Start Menu
  shortcut with the AppUserModelID, `shortcut.rs`; (2) WebView2 never hands a web notification
  to Windows on its own, so the host now handles `NotificationReceived` and raises the toast;
  (3) the permission is pre-granted on the profile so `Notification.permission` reads
  "granted" before the page asks. Before (2), the page's `onshow` fired and Windows' own
  notification database recorded nothing: the old "notifications work" claim was the web
  layer talking to itself.
- **Service-worker notifications never reach the host on Windows**, single- or
  multi-process. WhatsApp Web's loaded code uses `new Notification(` (bridged) and has one
  `showNotification` reference, so that residual path is real but small. See FINDINGS.md.
- The shortcut is named **"WhatsApp Rs"** because the C# original owns `WhatsApp.lnk` on
  this machine and rewrites it when the target differs. The Start Menu entry name and the
  toast's app name are that file name; Michael owns that decision.
- Memory, combined binary, 30 s settle: light 27.5 MB working set (3.7 MB private), safe
  426.6 MB across 3 processes. The picker still says "about 12 MB" for light mode, which was
  the standalone probe's number. Michael's call whether the text changes.

**Not verified, and worth being honest about:**
- **macOS is completely unbuilt.** wry's WKWebView backend does not implement the
  notifications permission at all (see wry's own `src/permissions.rs`), so Mac needs a
  hand-written native bridge. No Mac was available.
- Light mode has never been paired to a real account, by design.
- The Servo patches were never submitted upstream.

## Known gaps and traps

- **Linux has no voice/video calls**, because most distros build WebKitGTK without WebRTC.
  Measured here and confirmed by other projects. Linux is messaging-only, by decision.
- **No tray icon on stock GNOME Wayland** without a user-installed extension.
- **whatsapp-rust needs a nightly compiler by default.** Its `simd` feature gates
  `feature(portable_simd)`. Already worked around: `default-features = false` plus the rest
  of the default set by name. If a future version puts nightly-only code outside that flag,
  we are pinned to an older release.
- **Do not use `cargo fmt` bare in the Servo repo**, it reformats 400+ unrelated files.
  Servo needs specific unstable rustfmt args; see `SERVO_BUILD.md`.

## Michael's working preferences (obey these)

- **Replies must be short.** He stopped reading long ones. Lead with the answer, plain
  English, no jargon, no narration of how you got there.
- **Measure, never assert.** He caught me several times stating things from memory that were
  wrong. Every number must come from a command run in that session.
- **He hates Chrome, Edge, Opera, Firefox and the official WhatsApp app** (386 MB, "a
  ridiculous monstrosity"). Never propose the official app.
- **He owns all UI/UX decisions.** If he says it looks wrong, it is wrong. Implement his
  visual preferences literally.
- **Build from scratch, do not fork.** He explicitly did not want another project's code
  (`whatRust`). Using libraries is fine; copying an app is not.
- Do not open a visible console window; detached runs go hidden with output to a log.

## The immediate next steps, in order

1. ~~Run the app and click through the picker.~~ Done 2026-09-07; it had never started at
   all (see above). `tools/picker-drive.ps1 -Action Safe|Light|Cancel|Escape` re-runs it.
2. ~~Create the Start Menu shortcut and confirm a real toast.~~ Done 2026-09-07, on screen.
   `tools/toast-test.ps1` re-proves it in about 40 seconds. What remains open on that front
   is Michael's call on two names: the Start Menu entry / toast app name ("WhatsApp Rs"),
   and the picker's "about 12 MB" (measured 27.5 MB for the combined binary).
3. ~~Build a chat UI for light mode.~~ Functionally done 2026-09-07, visually rejected.
   `src/chat_ui.rs` (plain Win32 controls) + `src/chats.rs` (store, persisted as JSON):
   chat list with unread counts, message view, send box, Enter sends, close-to-tray, the
   pairing QR drawn in the window (no console needed any more), history sync and contact
   names imported, toasts only when the window is not in front. Verified by
   `tools/light-drive.ps1`: a real click, a typed message, the reply, at 19 MB working set.
   `--light-demo` shows it with sample chats and no network, which is the only way Michael
   will ever see it, since he will not link his number. His ruling on seeing the first,
   bare-controls version: DECISIONS.md #11, it must look like WhatsApp or light mode is off.
   **Restyled the same day** (`src/ui_theme.rs`, `src/ui_chatlist.rs`, `src/ui_messages.rs`,
   `src/chat_ui.rs`): WhatsApp Web's layout and palette in light and dark, following the
   Windows setting (`WHATSAPP_RS_THEME=light|dark` forces one), drawn with GDI/GDI+.
   Measured 20.2 MB working set, one megabyte more than the bare version. Screenshots of
   both themes and a send round trip come from `tools/light-drive.ps1 -Theme dark|light`.
   Michael has not yet seen the restyled version; the next session should open
   `whatsapp.exe --light-demo` for him first thing. Known gaps: text only (media shows as
   "[attachment]"), no read receipts sent, no typing indicators, no group member list, no
   bubble tails, time sits on its own line under the text rather than floating right of the
   last line, fonts are sized at launch DPI only, Shift+Enter does not insert a newline.
3b. **Safe mode on Servo, in progress (DECISIONS.md #12, the ruling that matters most now).**
   Michael saw safe mode listed as "Microsoft Edge" on his phone and ruled, angrily, that the
   engine must be the same on every OS and ours to run: Servo, embedded, presenting as Firefox.
   Done so far: `src/servo_view.rs` (winit window, Servo embedding API, input forwarding,
   toasts via the delegate, close-to-tray, geometry, single instance), the `servo` Cargo
   feature with a `servo` build profile (no LTO), `tools/build-servo.ps1` (mach's Windows
   environment, ANGLE DLLs copied beside the exe), and in the Servo fork: rusqlite moved to
   0.39 with a vendored `third_party/sea-query-rusqlite`, because light mode's session store
   links the same native SQLite and Cargo allows one copy (fork commit `42ef2253c`), and
   `rustls` with `aws-lc-rs` installed at startup, without which Servo's network thread
   panics silently. **Proven: the embedded build draws WhatsApp's real login page with a
   live QR, one process, 442 MB working set** (FINDINGS.md). NOT yet proven: the phone
   listing it as Firefox (needs Michael to link), login persisting across restarts
   (cookies flush on a clean quit through the tray menu; IndexedDB and Cache Storage write
   live under `<data>/servo/`), clipboard paste (no clipboard delegate yet), the toast path
   through Servo's Notification delegate, Mac and Linux builds. Build:
   `tools/build-servo.ps1` under `fairjob.ps1`, output `target/servo/whatsapp.exe` with the
   two ANGLE DLLs beside it; do not run it beside a normal `cargo build`, the target lock
   serialises them. Test: `tools/safe-drive.ps1`, which captures the engine log, the thing
   that found both silent failures.
   A leftover: `target/release/whatsapp.running.exe` is his running instance's binary,
   renamed out of the way so a build could write; delete it once he has quit that instance.
3c. **The chat-load stall, diagnosed and fixed 2026-09-07 (unverified against a real login).**
   Michael logged in on the Servo build, his phone showed **Firefox**, and the page then stuck on
   "Loading your chats". Root cause: WhatsApp's backend worker calls `navigator.locks.request()`
   unguarded on its first line, Servo had no Web Locks, so the worker threw before registering its
   message handler and the page waited on it forever. Web Locks is now implemented in the fork
   (`441e31675`, exported as `servo-patches/0005-*`), exposed on Window and **WorkerNavigator**.
   Twelve engine prefs that WhatsApp needs are now set by name, page-side polyfills cover
   `requestIdleCallback`, and the page's console is finally forwarded to stderr.
   **What is NOT verified: that the chat list now loads.** That needs Michael to scan the QR again.
   The app is running on his real profile with `WHATSAPP_RS_PROBE=1`, and its log
   (`scratchpad/session.err`) will capture the next login attempt.
   **The next blocker was fixed too, without waiting for it to bite:** `IDBCursor` had no
   `continue`/`advance`/`continuePrimaryKey`, so a cursor could read one record and never move.
   Implemented (`4acad3532`, `servo-patches/0006-*`) and verified at runtime: a five-record walk,
   `advance(2)`, `continue(key)`, and the double-continue error case all behave to spec.
   Still missing and not cheaply fixable: **OPFS** (`navigator.storage.getDirectory`), and
   `IDBCursor.update`/`delete`.
   ⚠ Servo writes the cookie jar **only on a clean shutdown**. Killing the process loses the login.
   Quit from the tray. (This is how a logged-in session was lost while debugging today.)
4. Optional: submit the five Servo commits upstream. They are exported as patch files in
   `servo-patches/` with a ready-to-paste PR description. Needs his GitHub account.
5. Nothing is committed: the repo has no commits yet (`git log` is empty). Nothing has been
   pushed anywhere. Commit when he says to.

## Reference docs already written

In `D:\NEWProjects\WhatsAppRs`: `README.md`, `FINDINGS.md` (all measurements),
`DECISIONS.md` (his rulings), `ENGINE_SURVEY.md` (why no lightweight engine works),
`ARCHITECTURE_DECISION.md` (where the memory goes), `SERVO_BUILD.md` (how to build Servo
on this machine), `servo-patches/PR_DESCRIPTION.md`.
