# Rust cross-platform WhatsApp wrapper: measured findings

Date: 2026-09-06. Every claim below was produced by a working Rust probe in this repo, run on
Windows natively and on Linux in a container. Nothing here is from memory or from research.
Where something is unverified, it says so.

## Verdict per platform

| Platform | Engine wry uses | Verdict | Basis |
| --- | --- | --- | --- |
| Windows | WebView2 (Chromium 152) | **GREEN, proven** | Loads, notifications working after a host-side fix |
| Linux | WebKitGTK 2.50.6 | **GREEN, proven** | Loads with no UA spoof, notifications working after a different host-side fix |
| macOS | WKWebView (Safari/WebKit) | **AMBER, untested** | Strong indirect evidence, but no Mac available to test on |

The macOS evidence is genuinely strong but is not proof. On Linux, WebKitGTK presented the UA
`Macintosh; Intel Mac OS X 10_15 ... AppleWebKit/605.1.15 ... Safari/605.1.15`, and WhatsApp
accepted it and served the **Mac** download banner. WKWebView is the same WebKit family presenting a
very similar Safari UA, so WhatsApp almost certainly accepts it too. Someone still has to run it on
a Mac.

## The engine honesty note

There is no embeddable engine in 2026 that renders web.whatsapp.com except a Chromium or a WebKit.
So "small" applies to **our binary and our profile directory** and can never apply to the rendering
engine, which is either the OS's own (free, already installed) or bundled (large). Any footprint
number must say which it counts.

On all three targets wry uses the **OS's own webview**, so the app bundles no engine:
Windows has the WebView2 Runtime preinstalled, macOS has WKWebView in the OS, Linux has WebKitGTK
from the distro. That is the design that keeps this small.

## Windows, measured

Head to head, same URL, fresh profiles, 30 seconds to settle, processes attributed by their
user-data-dir so other apps on the box were never counted.

| | Processes | Engine RAM | Rust shell | Total RAM | Disk (fresh) |
| --- | --- | --- | --- | --- | --- |
| Rust + WebView2 | 7 | 600.8 MB | 23.0 MB | **623.8 MB** | **36.1 MB** |
| Chrome `--app` (the C# app today) | 10 | 802.9 MB | n/a | **802.9 MB** | **102.9 MB** |

Projected steady state with real message history: about **92 MB** on disk against today's
**342.8 MB**, because a WebView2 profile creates none of Chrome's six browser-only folders. I
verified all six absent: Safe Browsing, optimization_guide_model_store, component_crx_cache,
WasmTtsEngine, OnDeviceHeadSuggestModel, ActorSafetyLists.

So about **73% less disk and about 22% less RAM**. Real, worth having, and not an order of
magnitude. The probe binary is 2.17 MB debug, versus 96 KB for the C# original, so the executable
gets bigger. All the savings are in the profile.

## Web API support, measured per engine

| API | Windows / WebView2 | Linux / WebKitGTK 2.50.6 |
| --- | --- | --- |
| Service Worker registered AND controlling | yes | yes |
| IndexedDB | yes | yes |
| SubtleCrypto | yes | yes |
| WebAssembly | yes | yes |
| SharedWorker | yes | yes |
| mediaDevices | yes | yes |
| **OPFS** (Origin Private File System) | yes | **NO** |
| **WebRTC** (RTCPeerConnection) | yes | **NO** |

The two Linux gaps are the real cross-platform risk:

- **WebRTC absent** means voice and video calls cannot work on Linux. Might be this container's
  WebKitGTK build rather than WebKitGTK in general, so it needs confirming on a real desktop.
- **OPFS absent** does not break anything today, because WhatsApp Web stores its data in IndexedDB.
  It is a future-breakage risk if WhatsApp starts requiring it.

Neither stopped the app loading or the service worker running.

## Notifications: two different traps, both solved

This is the part that would have silently broken a messaging app. Both engines **deny notifications
by default**, and each needs a completely different host-side fix.

**Windows.** WebView2 auto-denies. Measured `permissionAfter: "denied"`, and WhatsApp's
service-worker path then fails with *"No notification permission has been granted for this origin."*
The fix is `add_PermissionRequested` on the raw `ICoreWebView2`, granting
`COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS`. After the fix, `permissionAfter: "granted"` and an
isolation test proved service-worker notifications genuinely display:

```
{"tag":"swOnly","permission":"granted","showResolved":true,"displayedCount":1,
 "displayedTitles":["SW-ONLY-PROBE"],"verdict":"SW NOTIFICATION IS LIVE/DISPLAYED"}
```

This contradicts Microsoft's own documentation, which says WebView2 supports non-persistent
notifications only. The research pass read those docs and called service-worker notifications a hard
blocker. Direct measurement says otherwise. `NotificationReceived` fires on the host for
**page-context** notifications only, so Rust can restyle those and WebView2 displays the
service-worker ones itself.

**Linux.** WebKitGTK also denies, but the `permission-request` signal **never fires at all**, because
it auto-denies before asking. Connecting that signal is a dead end. The real fix is
`WebContext::initialize_notification_permissions` with a `SecurityOrigin::for_uri` for
`https://web.whatsapp.com`, which requires the `v2_16` feature on the webkit2gtk crate. After that,
both page and service-worker notifications display.

**The nasty Linux quirk, and it needs a shim.** After seeding, `Notification.permission` correctly
reads `"granted"` and notifications work, but `Notification.requestPermission()` still **resolves
with `"denied"`**. Measured in one run: `permissionBefore: "granted"`, `permissionAfter: "denied"`,
while `swShowOk: true` and the notification displayed. If WhatsApp Web gates its notification
feature on `requestPermission()`'s return value, it will disable alerts that would actually have
worked. Mitigation is a small injected JS shim overriding `Notification.requestPermission` to resolve
with the real `Notification.permission`. This is not yet implemented or tested.

**Still unverified on both platforms:** `getNotifications()` proves the web layer considers the
notification shown. I did not visually confirm an OS toast appearing. On Windows 11 a toast generally
needs a registered Start Menu shortcut carrying an AppUserModelID, which is exactly what the C#
original's `Shortcut.cs` and `TrayPromotion.cs` do, so that machinery must be kept rather than
deleted as legacy cruft. On Linux a toast needs a freedesktop notification daemon, which a container
does not have.

## What the rebuild deletes

Owning your own window collapses most of the C# original:

| C# class | Fate in Rust |
| --- | --- |
| `ChromeFinder` | **Gone.** No foreign executable to locate. |
| `WindowFinder` | **Gone.** You own the HWND; no title-sniffing another process's windows. |
| `Hooks` | **Gone.** The global mouse and keyboard hooks collapse to one close-event match arm. |
| `ProfileSeeder` | Mostly gone. Neither WebView2 nor WebKitGTK has Chrome's first-run UI to suppress. |
| `Geometry` | Survives, simpler, and fixes the every-1.5-seconds disk write bug. |
| `Shortcut` / `TrayPromotion` | Keep on Windows. They are what makes toasts appear. |
| `TrayHost` | Port to `tray-icon` + `muda`. |

Three of the four bugs found in the C# version disappear for free, because they live in code that
does not survive the port.

## Versions, live-checked 2026-09-06

`wry` 0.56.1, `tao` 0.37.0, `tray-icon` 0.24.2, `tauri` 2.11.5.

Per-platform raw handles, both confirmed by reading wry's own source rather than docs:
`WebViewExtWindows::webview()` gives `ICoreWebView2`; `WebViewExtUnix::webview()` gives
`webkit2gtk::WebView`. `with_user_agent` is on the base builder, so UA override works everywhere.
wry 0.56.1 resolves `webkit2gtk` 2.0.2 with `soup3` 0.5.0, i.e. the **webkit2gtk-4.1 ABI**, which is
what to install as a build dependency on Linux.

Two dependency traps hit and solved:

- Pin `windows` to **0.61** to match `webview2-com` 0.38.2. Pinning 0.62 pulls a second
  `windows-strings` and `PWSTR` silently becomes a different type, so the notification hook will not
  compile.
- Windows-only and Linux-only deps must be **target-scoped** in Cargo.toml
  (`[target.'cfg(target_os = "...")'.dependencies]`), or the other platforms fail trying to compile
  bindings they cannot use.

## Prior art: whatRust, and why it matters

`github.com/karem505/whatRust`, verified live: Rust + Tauri, **MIT**, 54 stars, 13 forks, created
2026-05-30, last pushed 2026-08-27, not archived, 9 open issues. Self-described as a low-RAM
alternative to the official WhatsApp Desktop, for Linux, Windows and macOS. It is essentially the
app being specified here, already built, with Flatpak, NSIS, MSI and AppImage packaging done.

**Its number one open bug is the exact bug solved in this repo today.** Issue #3, "Notification
Doesn't Appear", opened 2026-06-04, still open, reported broken across v0.3.1, 0.3.2, 0.3.3, 0.4.2,
0.5.0, 0.6.2 and 0.6.3. Four months, eight releases, five separate users. The maintainer's own log
output shows `commands::notify invoked` never fires at all, meaning the notification never reaches
the Rust host. He has been chasing AppUserModelID registration, which is the **display** side, while
the actual failure is upstream on the **source** side: WebView2 auto-denies the notification
permission, so WhatsApp's `showNotification()` fails inside the page and nothing is ever emitted.

The fix measured in this repo, `add_PermissionRequested` granting
`COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS`, addresses precisely that. This is a real, contributable
fix for a real, four-month-old bug affecting a live project.

Its other open issues corroborate findings here:

- **#15**: spoofing a Linux user agent removed the voice and video call buttons on Windows. Confirms
  the conclusion above that a UA override should not be the default.
- **#22 / #23**: AppImage packaging bundles a Wayland library that breaks Mesa EGL and yields a blank
  window; WhatsApp link handling is broken on Linux. Linux packaging is where the real work is.

## The account-suspension report, stated with its confounder

whatRust issue **#17**, opened 2026-08-05, zero replies: a user reports that each time they appeal
their suspended WhatsApp account, regain it, then link a desktop device via whatRust, the account
immediately re-enters "account in review". They say confirmed twice, personal WhatsApp only, Business
unaffected.

**SUPERSEDED 2026-09-06. This is almost certainly not caused by the wrapper.** Verified against
primary sources: WhatsApp's Terms of Service contain **no restriction on which browser you use**, and
Chrome, Firefox, Edge, **Opera** and Safari are all on WhatsApp's own supported-browser list. The
clause that actually governs bans is *"reverse engineer, alter, modify, create derivative works from,
decompile, or extract code from our Services"*, which describes a protocol reimplementation
(whatsmeow, Baileys, whatsapp-rust) or a modified WhatsApp app (GB WhatsApp, WhatsApp Plus). A
webview pointed at Meta's real website touches none of it, and is identical in kind to opening
web.whatsapp.com in Opera.

The reporter's account was **already suspended and under repeated appeal** before whatRust entered
the picture. An already-flagged account being re-flagged is the far better explanation. Recorded here
for completeness, but it should not be read as evidence that wrappers carry ban risk. They do not.

The full reasoning is in ARCHITECTURE_DECISION.md under the CORRECTION heading.

## Honest note on the research

Two research fan-outs were run. The first had 4 of 6 agents return stub summaries; its one confident
"hard blocker" was then disproved by measurement. The schema was hardened with minimum lengths and
item counts for the second run. **The probe, not the research, is what this document rests on.**

## Reproducing any of this

- Windows: `cargo build`, run the exe, read `probe-report.jsonl`.
- Linux: `docker build -f Dockerfile.linux-probe -t whatsapp-linux-probe . && docker run --rm whatsapp-linux-probe`.
- The RAM and disk comparison is `headtohead.ps1` in the session scratchpad.

## Toasts on Windows, measured 2026-09-07

Instrument: `tools/toast-test.ps1`. It starts safe mode with a WebView2 debugging port, fires a
notification inside the real WhatsApp page over that port, screenshots the toast corner, and reads
Windows' own notification database (`wpndatabase.db`), which records every toast Windows accepted and
under which app identity. A PowerShell-fired toast calibrated both instruments first.

| what fired | page's own report | Windows database | on screen |
| --- | --- | --- | --- |
| `new Notification()`, before the bridge | `onshow` fired | **no row** | nothing |
| service worker `showNotification()`, before the bridge | "displayed" per `getNotifications()` | **no row** | nothing |
| `new Notification()`, with the bridge | `onshow` fired | row under `com.lunarwerx.whatsapp-rs` | **toast drawn, titled "WhatsApp Rs"** |
| service worker `showNotification()`, with the bridge, single-process | "displayed" | no row | nothing |
| service worker `showNotification()`, with the bridge, multi-process | "displayed" | no row | nothing |

So the web layer's "displayed" is worth nothing on Windows: WebView2 does not raise a toast for a web
notification by itself, and the previous "notifications work" claim rested entirely on that report.
What works is the host handling `NotificationReceived` (ICoreWebView2_24), marking it handled, and
raising the toast through `notify::toast` (notify-rust) under the app's AppUserModelID, with the
Start Menu shortcut carrying that ID (`shortcut.rs`). The same `toast` path is what light mode uses.

Two more facts from the same run:

- **The permission has to be pre-granted, not just answered.** With only the `PermissionRequested`
  handler, `Notification.permission` read `"default"` until the page asked, and a `new Notification()`
  before that raised `onerror`. `ICoreWebView2Profile4::SetPermissionState` for the origin makes it
  read `"granted"` from the first paint (`permissionBefore: "granted"` measured after the change).
- **WhatsApp Web's own code uses the page path.** On the QR screen the page has loaded 24 scripts,
  26.7 MB; exactly one contains `new Notification(` (once) and `showNotification` (once); `sw.js`
  contains neither. The bridged path is the one WhatsApp uses.

Also measured the same day, because the app had never actually been launched with the picker in it:
the release exe exited in 1.4 s with **STATUS_ENTRYPOINT_NOT_FOUND (0xC0000139)**. `TaskDialogIndirect`
lives only in Common Controls v6, and an exe with no manifest gets v5. `build.rs` now embeds the
manifest (`embed-manifest` 1.5.0); all three picker buttons and Escape were then driven through UI
Automation and behaved (`tools/picker-drive.ps1`).

Memory, same method as above (30 s settle, whole process tree), combined binary:

| mode | processes | working set | private |
| --- | --- | --- | --- |
| light | 1 | 27.5 MB | 3.7 MB |
| safe | 3 | 426.6 MB | 237.3 MB |

The picker still says "about 12 MB" for light mode. That figure was the standalone probe; the combined
binary measures 27.5 MB working set with 3.7 MB private. Whether the picker text changes is the
owner's call.

## Servo presenting as Firefox, measured 2026-09-07

The fork's `servoshell` (branch `cache-storage-complete`), headless, `--config-dir` set, the three
WhatsApp prefs on, and `-u "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:143.0) Gecko/20100101
Firefox/143.0"`: WhatsApp serves the real login page (heading, the three steps, the phone-number
login link, the create-account form). No "update Firefox" gate this time; the earlier survey hit it
because Servo's default string carries an old Firefox version. WhatsApp's JS logs one
`[page load] validation failed (page-load-validation)` error and carries on. The QR square was still
blank at the moment `-o` captured (about five seconds in); whether it fills is what the embedded
build answers. Working set of that headless run was not measured; the earlier figures are #10 in
DECISIONS.md.

## Safe mode on Servo, embedded, measured 2026-09-07

`tools/build-servo.ps1` then `tools/safe-drive.ps1` (scratch profile, 40 s wait). The app's own
window, Servo inside it, presenting as Firefox 143:

| what | result |
| --- | --- |
| page | WhatsApp's real login page, **QR drawn** (`servo-safe.png`); load status Started, HeadParsed, Complete in the engine log |
| processes | **1** |
| working set / private | **441.9 MB / 533.5 MB** |
| binary | 137 MB exe plus `libEGL.dll` and `libGLESv2.dll` beside it |
| profile after 40 s | 31 files, 4.5 MB under `<data>/servo/` (IndexedDB, client storage, cache) |

Two things it took to get there, each a silent failure until the engine log was captured:

- **rustls has no crypto provider unless the embedder picks one.** The network thread panicked on
  its first TLS handshake ("Could not automatically determine the process-level CryptoProvider")
  and the window stayed white at 77 MB. `rustls` with `aws-lc-rs` in Cargo.toml plus
  `install_default()` at startup, the same choice servoshell makes.
- **Two crates linking the native SQLite.** Servo's net crate wanted `libsqlite3-sys` 0.36 through
  rusqlite 0.38; light mode's session store wants 0.37. Cargo allows one. The fork now pins rusqlite
  0.39 with a vendored `sea-query-rusqlite` (no release on 0.39 yet), commit `42ef2253c`.

Noise in the log that is not a fault: "Empty hit test result" and "Unknown pipeline" while the
mouse moves over the window before the page exists; one WebSocket `ResetWithoutClosingHandshake`,
which is WhatsApp's QR socket cycling.

## Why Servo logged in and then stalled on "Loading your chats", 2026-09-07

Michael scanned the QR, his phone listed the device as **Firefox**, and the page then sat on
"Loading your chats" and never advanced. Login had worked and IndexedDB was filling, so the socket
and the main thread were both fine. The cause was on the other side of a worker.

**The embedder was deaf.** `WebViewDelegate::show_console_message` was never implemented, so every
error the page logged went nowhere. That is why the stall looked silent. Implementing it, plus a
probe that asks the page what it has (`WHATSAPP_RS_PROBE`), turned this from guesswork into
measurement, and everything below came from it.

**Servo keeps most web APIs behind preferences that default to off**, and safe mode was setting only
three. Measured on the login page, all of these were undefined: `IntersectionObserver`,
`navigator.storage`, `navigator.permissions`, `OffscreenCanvas`, `FontFace`, `Element.animate`,
`CompositionEvent`, `visualViewport`. A virtualised chat list cannot render without the first.
Twelve prefs are now set by name in `servo_view.rs`.

**Three APIs were absent entirely, not just switched off**: `requestIdleCallback`, `navigator.locks`
(Web Locks) and `navigator.storage.getDirectory` (OPFS).

**The Web Locks one is what hung the app.** WhatsApp's backend worker script calls
`self.navigator.locks.request(name, {steal: true}, ...)` unguarded, and the statement that registers
its message handler is the last line of the file. With no `navigator.locks` that call throws a
synchronous TypeError, so the handler is never registered, the worker answers nothing, and the main
thread's boot waits on it with no timeout and no error handler. Every other `navigator.locks` call
in WhatsApp's 13 MB of bundles is null-guarded; the worker's is the only one that is not.

Fixed by implementing Web Locks in the fork (commit `441e31675`): `LockManager` and `Lock`, exposed
on Window **and WorkerNavigator**, with a process-global registry keyed by origin so a window and
its workers see the same locks, and cross-thread waking so a lock released on one thread can be
granted to a waiter on another. Verified in the generated bindings that `WorkerNavigator.Locks`
exists, which is the global that matters.

A page-side polyfill would not have been enough: **Servo's user scripts are injected into documents
only, never into workers**, so an embedder cannot patch a worker global from outside. That is why
the fix had to go into the engine. `requestIdleCallback` is window-only in every browser, so a page
polyfill is sufficient there and that is what it gets.

Still missing after all of this: **OPFS**. `StorageManager` in Servo has only `persisted`, `persist`
and `estimate`; there is no `getDirectory` and no file-system-handle machinery behind it. It cannot
be polyfilled cheaply.

**The next blocker was already visible, and is now fixed too.** `IDBCursor` declared only readonly
attributes: no `continue`, `advance`, `continuePrimaryKey`, `update` or `delete`. A page could open
a cursor, read its first record, and never move past it, which is fatal for syncing history. The
iteration algorithm itself was already present and correct; what was missing was the plumbing from
the DOM methods into it. Implemented in commit `4acad3532` and verified at runtime against a real
store:

| test | result |
| --- | --- |
| walk five records with `continue()` | visits all five in order, then ends |
| `advance(2)` | visits 1, 3, 5 |
| `continue(4)` then `continue()` | jumps to 4, then 5 |
| `continue()` twice without waiting | throws `InvalidStateError`, as the spec requires |

`update()` and `delete()` on a cursor are still unimplemented.

## WhatsApp Web fully loaded on Servo, 2026-09-07

After the QR, the page reached "Loading your chats" and stopped there. Three engine gaps in a row
caused it, each hidden behind the last:

1. **Web Locks.** WhatsApp's backend worker calls `navigator.locks.request()` unguarded on its first
   line, before it registers its message handler. Servo had no Web Locks, so the worker threw, went
   silent, and the page waited on it forever. Implemented in the fork (`441e31675`). After this the
   worker starts and replies, and the page gets past the QR.
2. **IndexedDB cursors could not move.** `continue`, `advance` and `continuePrimaryKey` were absent,
   so a cursor read one record and stopped (`4acad3532`).
3. **IDBIndex had no methods at all.** `getAll` and `openCursor` on an index threw
   "is not a function", which is exactly what the page's own error handler reported once the
   embedder started forwarding the console. Implemented, along with a real bug in Servo's cursor
   iteration that left an index cursor's primary key undefined (`df26942dc`).

With those, **the chat list renders**: real conversations, avatars, previews and unread counts
(`chats-loaded.png`). One process.

### Memory with a real account, and the leak that was in my own patch

Measured on the same account, same launch, same intervals:

| | 1 min | 2 min | 4 min | later |
| --- | --- | --- | --- | --- |
| first working build | 949 MB | 959 MB | 1643 MB | 2022 MB after ~45 min |
| after the fix below | 1400 MB | 1433 MB | **1436 MB, flat** | |

The growth was mine. An index query rebuilds its records from a full store scan, and my first
version cloned every stored value into every synthesized record, including for `getAllKeys`,
`count` and key-only cursors that never read a value, multiplied again by array length for a
multiEntry index. Skipping the copy when the caller does not need values (commit `bb675ad84`)
stops the climb.

Servo's own memory settings then took another 250 MB off. Compacting and incremental garbage
collection are both off by default, so the JavaScript heap was never handed back; with those on
plus one layout thread it settles at **1168 MB, flat**, and the chat list still renders correctly
including pinned and archived rows. Those three are now set in `servo_view.rs`, so a plain launch
gets them.

**It is still about 1.2 GB, and that is the open problem.** For comparison, the Chrome wrapper this
replaces measured 803 MB on the same account, and DECISIONS.md #10 records that even 500 MB was
judged too much. Two candidates for the remainder, in order: index queries still materialise the
entire object store as JavaScript values on every call, which is the design's known weakness and
the reason real index tables in the backend are the destination; and Servo's own footprint on a
page this size. Neither has been measured apart yet.

The instrument that made this tractable was forwarding the page's console and capturing unhandled
rejections **with their message**, not just a stack. The first attempt logged only stacks and named
nothing; adding `name: message` turned a day of guessing into two lines that said exactly what was
missing.

Still open after all this: **OPFS** is absent and cannot be shimmed cheaply, and one
`DataError: Provided data is inadequate.` rejection still appears during load without visibly
breaking anything. `IDBCursor.update`/`delete` remain unimplemented, and index queries scan the
whole object store, which will not scale to a large mailbox.

## Why it feels slow, measured 2026-09-07

The owner asked whether this is a cut-down WhatsApp. It is not: it is the same web.whatsapp.com any
browser loads, and nothing is stripped. The slowness is the engine.

**Rendering is properly GPU accelerated**, so this is not a software-rendering fallback:
`webrender` reports `ANGLE (NVIDIA GeForce RTX 4070 Ti Direct3D11)` through OpenGL ES 3.0.

**Frame timing on the real account** (`tools/jank-run.ps1`, six seconds of animation frames taken
thirty seconds after the chat list appears):

| configuration | fps | median frame | 95th percentile | worst |
| --- | --- | --- | --- | --- |
| compacting + incremental GC, 1 layout thread | 18.4 | 31.6 ms | 164 ms | 211 ms |
| engine defaults + incremental GC | 38.1 | 16.4 ms | 42 ms | 348 ms |
| 6 layout threads | 31.5 | 31.3 ms | 33 ms | 47 ms |
| engine defaults | 31.2 | 31.4 ms | 46 ms | 46 ms |
| engine defaults, repeat run | 8.5 | 85.2 ms | 278 ms | 324 ms |

**Read that last row before drawing conclusions from the others.** The same configuration measured
8.5 fps and 31.2 fps on two runs, so run-to-run variance is larger than most of the differences
here. What survives the noise: the first row is genuinely bad, and it was a self-inflicted wound.
Forcing compacting collection and a single layout thread, which had looked like a memory win, cost
roughly half the frame rate and saved nothing; both overrides are reverted, and the code now says
not to tune this without the probe.

What does not survive: any claim that a particular pref makes it fast. It is slower than Chromium
because Servo's layout and script are younger, and at 30 fps with occasional 300 ms stalls that is
visible. Memory sits around 1.2 GB across runs, against 803 MB for the Chrome wrapper this replaces.

**Index queries are not the cause of the lag**, though they will be eventually. Benchmarked over a
3,000-record store with an index (`tools/perf-test.js`): one `index.getAll` returning 60 rows takes
7 ms, ten `getAllKeys` take 48 ms. That is a full scan every time, so it grows with the store; at a
mailbox ten times this size it becomes tens of milliseconds per lookup and worth fixing properly.

## Storage persistence on Servo, measured 2026-09-07

Written in one run, read back in the next, with a clean shutdown between
(`WHATSAPP_RS_QUIT_AFTER` runs the same path as the tray's Quit):

| what | survived a clean restart |
| --- | --- |
| `localStorage` | yes |
| IndexedDB record | yes |
| WhatsApp's own HTTP cookies | yes, `cookie_jar.json` written at quit and read at start |
| a cookie set from JavaScript | no, it never reaches the jar |

The catch: Servo writes the cookie jar **only at shutdown**. Killing the process skips it and costs
the login. That is how a logged-in session was lost during this session's debugging, and it means
"quit from the tray" is currently load-bearing rather than a nicety.

Instrument: `tools/light-drive.ps1`. Plain Win32 controls (list box, two edit boxes, a button) and
GDI for the QR; no toolkit, no GPU.

| state | working set | private | threads |
| --- | --- | --- | --- |
| `--light-demo`, chats screen, one send round trip | 19.1 MB | 2.5 MB | 5 |
| `--light`, pairing screen with a live QR from WhatsApp's servers | 22.4 MB | 4.0 MB | 10 |

So the window itself costs nothing visible: the 27.5 MB above was the same binary before it had one.

Then the restyle (DECISIONS.md #11): WhatsApp Web's layout and palette, avatars, rows, pills and
bubbles drawn with GDI and GDI+ (anti-aliased shapes; gdiplus.dll is the one extra library), the
dark title bar through DWM, the theme read from the registry. Same instrument, same round trip:

| state | working set | private | threads |
| --- | --- | --- | --- |
| `--light-demo`, dark theme, chat open, one send round trip | 20.2 MB | 3.0 MB | 6 |
| `--light-demo`, light theme, same | 20.2 MB | 3.0 MB | 6 |
| `--light`, pairing screen, dark theme | 23.0 MB | 4.0 MB | 11 |

One megabyte for the whole look. The owner's guess ("I doubt some basic styling is going to
significantly destroy our memory usage") was right, and the reason is that GDI drawing is what the
bare controls were already doing; only GPU-composited toolkits carry a large fixed cost.

Second instrument lesson: an always-on-top overlay (class `dframe-sidebar`) hit-tests over the left
of this screen while drawing nothing visible, so clicks aimed at the chat list landed in it and the
screenshots looked fine. `AutomationElement.FromPoint` under the cursor is what caught it; the driver
now moves the window to a clear area first and prints what is under the cursor before clicking.

One instrument lesson: UI Automation's `SelectionItem.Select()` on a list box sets the selection
without the `LBN_SELCHANGE` a mouse click sends, so the app never saw it and the typed text went to
the list box's type-ahead until the letter m jumped it to "Mum". A real click (`mouse_event`)
reproduces what a person does; the driver now clicks.

---

# Bundled Chromium vs bundled Firefox, 2026-09-07

The head-to-head DECISIONS.md #15 asked for. Both are built, both run, both were measured on this
machine with the same instruments (`tools/engine-bench.ps1`, `tools/bench-summary.py`). Servo and
light mode are untouched and unmentioned below; they are retired.

## What each one IS, because the two are not the same kind of thing

**Chromium is embedded.** The `cef` crate (Chromium Embedded Framework, 152.0.0+152.0.5, tracking
Chromium 152.0.7977.54) links into our process, and the browser is created as a child window of the
window we already own. `src/cef_view.rs`. Everything the current design owns - tray, close to tray,
single instance, window geometry - carries over unchanged, because the arrangement is exactly what
WebView2 was: our window, someone else's renderer inside it.

**Firefox is driven.** Gecko has no embedding API, so a Firefox build means shipping a real Firefox
and adopting its window with `SetParent`. `src/firefox_view.rs`. That was the risk flagged in the
brief, and the first thing measured. **It works.** The Firefox window becomes a child of ours,
survives being reparented, resizes with us, hides to the tray with us, and keeps rendering: a
screenshot taken during the test shows WhatsApp Web inside a window whose title bar and border
are ours. (`tools/engine-drive.ps1 -Engine firefox` reproduces it; screenshots are gitignored.)
It is still a hack, and it brings back three things the current design had deleted:

- finding another process's window (the C# original's `WindowFinder`),
- making two processes die together (a job object with kill-on-close, so a crashed host cannot leave
  an orphan browser holding the profile lock),
- forwarding focus by hand, because activating our window does not give an adopted child keyboard
  focus and typing goes nowhere after a restore from tray.

None of those is hard. They are all in `firefox_view.rs` and they all work. The point is that they
exist at all: the Chromium build needs none of them.

## The findings that decide it, in the order they matter

### 1. Codecs: Firefox has H.264, our bundled Chromium does not

Measured with `tools/webrtc-probe.js` inside the real `web.whatsapp.com` page, on both builds:

| | RTCPeerConnection | ICE candidates | microphone | audio codecs | video codecs |
| --- | --- | --- | --- | --- | --- |
| bundled Chromium (CEF 152) | yes | 3 (host, srflx) | granted, real device | Opus, G722, PCMU, PCMA, CN, telephone-event | VP8, VP9, AV1, rtx, red, ulpfec |
| bundled Firefox 155.0.1 | yes | 13 (host, srflx) | granted, real device | Opus, G722, PCMU, PCMA, telephone-event | VP8, VP9, **H264**, AV1, ulpfec, red |

Both can do a call; Servo never could, which was one of the reasons it was retired. The difference
is **H.264**, and it is not a setting. The official CEF binary distributions on
`cef-builds.spotifycdn.com` are compiled without proprietary codecs, so there is no flag that turns
it on: getting H.264 into the Chromium build means building CEF from source with
`proprietary_codecs=true ffmpeg_branding=Chrome`, which is a full Chromium checkout and build.

Audio calls use Opus and both have it. Video is where this bites, and how hard it bites depends on
whether WhatsApp Web negotiates VP8 with phones or insists on H.264 - which needs a real call on a
linked account to answer, and is the single most useful thing to test next. Nobody has published an
answer: a search of Meta's own engineering posts, WhatsApp's blog, Bugzilla and Chromium's tracker
turns up no statement of which video codecs WhatsApp Web negotiates. (WhatsApp Web calling itself
only launched on 2026-07-28, which is part of why.)

**There is a second, already-documented consequence that does not depend on calls at all.** Other
people embedding CEF have hit exactly this against WhatsApp: without proprietary codecs, **uploading
an MP4 to WhatsApp fails** with "video not supported", because the page decodes the file locally
before sending it. That is a plain feature loss in the Chromium build, on a path a person uses far
more often than a video call, and it is the same single cause.

### 2. Firefox's permission prompts live in the browser UI we deleted

Found by measurement, not by reading: the first WebRTC probe on Firefox came back
`mic: "timed out waiting for the permission answer"`. Firefox asks for the microphone with a
doorhanger anchored to the URL bar, and the URL bar is exactly what `userChrome.css` collapses to
make the window look like an app. The prompt is raised into a bar that is not on screen, so nobody
can ever answer it, and a voice call would hang with no visible cause.

Fixed by pre-granting in the profile the app writes (`permissions.default.microphone`,
`permissions.default.camera`, `permissions.default.desktop-notification`,
`media.navigator.permission.disabled`), after which the same probe reports `mic: "granted"` with the
real device name. Worth stating plainly: these are `default` grants, i.e. every site, and they are
only acceptable because this profile is ours and the app opens exactly one URL.

The Chromium build has the same problem in principle and a better answer: CEF hands permission
requests to the host (`CefPermissionHandler::OnShowPermissionPrompt`), so `cef_view.rs` answers them
in code - accept for `web.whatsapp.com`, deny for anything else - with no UI involved at all.

### 3. CEF's Chrome runtime style crashes with a native child window

CEF 152 has two runtime styles. Chrome style is the one carrying Chromium's own notification and
permission machinery, so it was the obvious choice. **With a browser created as a child of a native
window it dies with an access violation (0xC0000005) immediately after `create_browser` returns 1**,
twice out of two, with nothing in `cef.log`. The identical build under Alloy style runs. Chrome
style wants CEF's own Views window, which is not a window we own, and owning the window is the whole
design.

So the build is Alloy, and Alloy has two costs. Its default answer to a permission prompt is
*ignore*, so a request the host does not answer hangs forever - that is what the permission
handler and the `SetContentSetting` call in `cef_view.rs` exist for. And, worse and separately,
Alloy turns Blink's notification support off outright, so granting the permission is not enough:
nothing is ever displayed. Finding 7 measures that and describes the bridge that fixes it.

### 4. Firefox removed the Chrome DevTools Protocol in 141

`--remote-debugging-port` still exists on Firefox 155 and now speaks **WebDriver BiDi**, not CDP,
and the `remote.active-protocols` pref that used to switch between them is gone. A CDP client
against it fails in a way that reads like a broken browser. `tools/bidi-eval.py` is the client for
Firefox; `tools/cdp-eval.py` remains the one for Chromium. Both must suppress the `Origin` header -
Firefox answers 400 and Chromium answers 403 to a websocket that carries one - and Firefox allows a
single BiDi session that it does not free the instant the socket closes, so a client that does not
call `session.end` breaks the next probe.

### 5. Disk: they are within 20 MB of each other, and both are ~10x the OS webview build

Measured by assembling the actual shippable folder with `tools/bundle.ps1` and then running the app
out of it, rather than by reading the build directory (which holds .pdb files, headers, import
libraries, a 20 MB CREDITS.html and 220 locales).

| bundle | size | files | what was dropped |
| --- | --- | --- | --- |
| Chromium, trimmed | **325.5 MB** | 11 | 219 locales, software-GL and DirectX-shader fallbacks, CREDITS.html, headers, .pdb |
| Chromium, with GPU fallbacks kept | 362.1 MB | 17 | as above, minus the fallbacks |
| Firefox, trimmed | **344.4 MB** | 59 | updater, crash reporter, maintenance service, default-browser agent, gmp-clearkey |
| (for comparison) the OS-webview build | 36.1 MB profile, ~2 MB exe | | nothing bundled: the engine is already on the machine |

The trimmed Chromium bundle was launched and rendered WhatsApp Web correctly, so the trim is
measured rather than a list of files that looked unused. The 36 MB fallback set (`vk_swiftshader`,
`d3dcompiler_47`, `dxcompiler`, `dxil`, `vulkan-1`) is software rendering and DirectX shader
compilation; dropping it is fine on this machine and is a real risk on a machine with no working GPU
driver.

Neither of these is small. That is what "bundle the engine" costs, and it is the honest price of
not being Edge.

### 6. What is NOT yet known, and needs the owner's account

Everything above was measured on a logged-out window, because a logged-out window is
repeatable and cannot cost a login. Three things cannot be answered that way, and all three
are things that would change the answer:

- **Memory with a real mailbox.** WhatsApp Web's footprint is mostly the synced account. The
  Chrome wrapper's 803 MB and Servo's 1.2 GB were both measured on the owner's real account;
  the numbers here are not comparable to those until the same is done for these two.
- **Responsiveness.** `frames.js` reports about 120 fps for both engines on the login page,
  which is the monitor's refresh rate and means nothing: there is nothing to paint. The probe
  scrolls the chat list on purpose, and there is no chat list until someone logs in.
- **A real call.** Both engines can gather ICE candidates and open the microphone. Whether a
  WhatsApp call actually connects, and which codec it settles on, is a question only a real
  call answers.

`tools/login.ps1 -Engine cef` and `tools/login.ps1 -Engine firefox` open each build on its own
persistent profile for a QR scan; each engine keeps its own login and neither touches the
Servo profile. After that, `tools/engine-bench.ps1 -Engine X -Runs 3 -Real` is the same
protocol against the real account.

### Method, and why two memory numbers are reported instead of one

Every figure below is the sum over the whole process tree, sampled a fixed time after the
PAGE reported itself ready rather than a fixed time after launch, because the two engines
reach a drawn page at different speeds and "60 seconds after start" would be comparing two
different moments.

**Working set** is what Task Manager shows and what every earlier number in this document
used, so it is the one that compares to the 803 MB Chrome wrapper and the 1.2 GB Servo build.
It counts a page of memory once per process that has it mapped, so it flatters an engine with
few processes and punishes one with many.

**Private bytes** is what each process cannot share with anything else. It is the honest
answer to "how much of this machine's memory does this app actually consume", and it is
reported beside the working set for every configuration. Where the two disagree, believe the
private number.

**CPU seconds** is cumulative processor time across the tree at the sample point. It answers
what neither memory number can: how much work the engine did to put the same page on screen.
On a 32-core machine an engine can be busy without ever feeling slow, and this is the only
figure in the run that notices.

### 7. Notifications: Firefox does it by itself, Chromium does not and needs the host

Tested with `tools/notify-test.ps1`, which reads **Windows' own notification database** rather
than believing the page. That distinction is the whole reason this was re-tested: on the
WebView2 round the page reported `onshow` and `getNotifications()` said "displayed" while
Windows had recorded nothing and no toast had appeared.

| | what the page reported | what Windows recorded |
| --- | --- | --- |
| bundled Chromium, host bridge OFF | `onshow` fired, service worker resolved, permission `granted` | **nothing** |
| bundled Chromium, host bridge ON | identical | **two toasts under `com.lunarwerx.whatsapp-rs`** |
| bundled Firefox | identical | **one toast under `FirefoxPortableToast-45C7D66DB3C307DD`** |

Both toasts were then confirmed **on screen**, not just in the database, and the screenshots
show the difference that matters more than which engine needed help:

| | toast header | toast body |
| --- | --- | --- |
| bundled Chromium + our bridge | **"WhatsApp Rs"**, the app's own name and identity | the notification's title and text |
| bundled Firefox, unaided | **"Firefox"**, with the Firefox logo | the text, plus "via web.whatsapp.com" |

So Firefox does it for free and gets it wrong: every WhatsApp message would announce itself as
Firefox in the Action Center. Chromium needed code and gets it right, because the host raises the
toast under the AppUserModelID `shortcut.rs` registers. Fixing Firefox's branding would mean
registering our own COM notification server and persuading Firefox to use it, which is more work
than the Chromium bridge that already exists.

The page's report is, once again, worth nothing, and the two engines differ:

- **Firefox raises a real Windows toast with no host code at all.** It registers its own COM
  notification server and files the toast under an identity derived from the install path
  (`FirefoxPortableToast-<hash>`), which is why it is branded Firefox.
- **CEF displays nothing.** Alloy style turns Blink's notification support off outright, and
  CEF exposes no callback that hands a notification's title and body to the host: `CefClient`
  declares eighteen handler factories and none of them is about notifications. So `cef_view.rs`
  bridges it: a script injected in the render process at document start replaces
  `window.Notification` and `ServiceWorkerRegistration.prototype.showNotification`, and the
  host raises the toast itself under the AppUserModelID that `shortcut.rs` registers - the
  same path the WebView2 build already needed.

Two things that only came out by measuring the shim rather than trusting it:

- **A plain `window.Notification = Shim` does not stick.** The shim demonstrably ran (its
  marker was on the page) and `Notification` was still the engine's own constructor
  afterwards. `Object.defineProperty` plus a re-check works, and the shim now logs
  `WHATSAPP_RS_SHIM installed=true` so a future failure is visible instead of silent.
- **Something on the page replaces it again after document start.** The re-check fires on
  every load: `WHATSAPP_RS_SHIM reinstalling, something replaced Notification`. Without the
  second install the bridge would have worked in a test and failed in the product.

### 8. `--single-process` on Chromium: 352 MB, one process, and nothing measurably broken

Chromium's `--single-process` is officially unsupported and puts the renderer in the browser
process, so a renderer crash takes the app down instead of showing a sad tab. The WebView2
build already accepted that trade. Measured on CEF it is worth more than it was there:

| | processes | working set | private | CPU to first paint |
| --- | --- | --- | --- | --- |
| bundled Chromium, default | 7 | 548 MB | 385 MB | 7 s |
| bundled Chromium, `--single-process` | **1** | **352 MB** | **294 MB** | 6 s |

Before trusting it, `tools/capability-probe.js` asked the page what it still had. Everything:
service worker **registered and controlling**, IndexedDB, SubtleCrypto, WebAssembly,
SharedWorker, Web Locks, OPFS, mediaDevices, WebRTC, WebGL, and the notification permission
reading `granted`. The WebRTC probe under single-process still gathered ICE candidates and
still opened the microphone with its real device name.

### 9. Neither engine is falling back to software rendering

Worth ruling out rather than assuming, because it would have explained both Firefox's memory
and its CPU. Asked of the page directly (`tools/gfx-probe.js`):

- Chromium: `ANGLE (NVIDIA, NVIDIA GeForce RTX 4070 Ti (0x00002782) Direct3D11 vs_5_0 ps_5_0)`
- Firefox: `ANGLE (NVIDIA, NVIDIA GeForce GTX 980 Direct3D11 vs_5_0 ps_5_0), or similar`
  (Firefox deliberately reports an approximate adapter for fingerprinting reasons; the point
  is that it is a Direct3D11 GPU path, not a software rasteriser.)

Both are hardware accelerated. Firefox's cost is Firefox, not a misconfiguration.

### 10. Memory: the tuning sweep

Two runs per configuration, logged out, sampled 45 s after the page reported itself ready. This
is the exploration pass that chose what to put in the head-to-head; the spread column is there
so a difference smaller than the noise is visible as such.

| engine | configuration | ready s | working set | private | processes | CPU s | profile MB | ws spread |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Chromium | default | 6.5 | 548 | 385 | 7 | 7 | 36 | 546-549 |
| Chromium | `--disable-gpu` | 3.3 | 527 | **294** | 7 | 7 | 33 | 522-532 |
| Chromium | `--renderer-process-limit=1` | 10.0 | 529 | 375 | 6 | 10 | 36 | 525-533 |
| Chromium | `--single-process` | 3.3 | **352** | **294** | **1** | 6 | 36 | 351-354 |
| Firefox | stock, no trimming | 3.2 | 1303 | 1174 | 14 | ~40 | 107 | 1303-1304 |
| Firefox | trimmed | 3.8 | 1186 | 1103 | 11 | 41 | 87 | 1183-1190 |
| Firefox | trimmed + `fission.autostart=false` | 3.3 | 1194 | 1107 | 11 | 39 | 127 | 1190-1198 |
| Firefox | trimmed + no fission + no GPU process | 3.8 | 1112 | 1030 | 10 | 41 | 88 | 1111-1113 |
| Firefox | `webContentIsolationStrategy=0` + no GPU process | 3.4 | 1141 | 1064 | 10 | 41 | - | 1117-1164 |
| Firefox | as above + no RDD process | 3.3 | 1124 | 1041 | 10 | 36 | - | 1113-1135 |
| *(control)* OS webview | as shipped today | - | 376 | 201 | 3 | 12 | - | 370-382 |

Read across that table and three things are settled:

1. **Firefox has a floor at about 1.1 GB and no pref moves it.** Six configurations, from stock
   to every process-reducing knob Mozilla still honours, span 1112-1303 MB. `fission.autostart`
   changed nothing measurable (1194 vs 1186, inside the spread), so either it is no longer
   honoured or it is not what keeps those processes alive. Turning off the GPU process is the
   only setting worth anything, and it is worth about 70 MB.
2. **Chromium is roughly half of Firefox before any tuning, and a third of it after.**
3. **Firefox does about five times as much work for the same page.** 36-41 CPU seconds against
   6-7, measured over a slightly *longer* window for Chromium since it took longer to be ready.
   Neither engine is software-rendering (finding 9), so this is simply what each costs. On a
   laptop that is the battery.

The measurements are also unusually repeatable - most configurations vary by under 10 MB
between runs - which is worth saying because the Servo round's numbers were not.

### 11. The control that proves the method

Every number above is "sum the working set and private bytes of the whole process tree", and
that method can be wrong in ways invisible from inside it. So the same script was pointed at
plain Chrome, `--app`, a scratch profile, logged out (`tools/chrome-control.ps1`):

| | processes | working set | private | profile |
| --- | --- | --- | --- | --- |
| plain Chrome, measured today | 10 | **800.4 MB** | 570.9 MB | 102.1 MB |
| the C# wrapper, measured 2026-09-06 | 10 | **802.9 MB** | - | 102.9 MB |

Two and a half megabytes and one hundred kilobytes apart, on different days by different
scripts. The method reproduces the number this whole project exists to beat, so the figures
beside it can be trusted.

It is also the comparison that matters most to the owner: what he runs today is 800 MB across
ten processes. The bundled Chromium in one process is **352 MB**, which is 44% of it.

### 12. Where that leaves the choice

Stated plainly, because the point of the exercise is a decision.

**Chromium wins on everything that was measured.** Half to a third of Firefox's memory, a fifth
of its CPU, one process against ten, a third of the profile on disk, and it embeds properly, so
the tray, close-to-tray, single instance and window geometry keep working with no new machinery.
At 352 MB in one process it is under the 500 MB the owner called too much (DECISIONS.md #10) and
is 44% of the 800 MB he runs today.

**One number does not go Chromium's way, and it should be said.** The OS webview - the design
being replaced - is still the lightest thing here on private bytes: 201 MB against the bundled
Chromium's 294 MB, though its working set is higher (376 vs 352) because it spreads across three
processes. Bundling an engine costs about 90 MB of real memory and 325 MB of disk over using the
one already on the machine. That is the price of not being Edge, and it is the price the owner
already decided to pay when he asked for this comparison.

**Firefox wins on two things, and one of them is not small.**

1. **H.264.** Firefox has it; the public CEF binaries do not and cannot be switched into it.
   That costs a WhatsApp video call that will not negotiate VP8, and it costs MP4 upload, which
   other CEF embedders have already hit against WhatsApp specifically. Both are fixable by
   building CEF from source with `proprietary_codecs=true ffmpeg_branding=Chrome` - a full
   Chromium build, so a one-off job on a machine with the disk for it, not a rebuild here.
2. **Notifications with no host code.** Firefox raises real Windows toasts by itself. This one
   is thinner than it looks: Firefox's toasts are branded **"Firefox"**, so every message would
   announce itself as the browser rather than the app, and Chromium's bridge - written,
   measured, and confirmed on screen - produces toasts headed **"WhatsApp Rs"**. Firefox saves
   the work and loses the branding.

**What Firefox costs, beyond the memory.** It cannot be embedded, so the app finds and adopts
another process's window; two processes must live and die together; keyboard focus has to be
forwarded by hand; and its permission prompts are raised into browser UI that an app window
does not have, which silently broke the microphone until the profile pre-granted it. All of
that works now. None of it exists on the Chromium side.

**The honest gap in this comparison**: everything above is logged out. The engine that looks
better on an empty login page is very likely the one that looks better on a synced mailbox, but
that has not been measured, and neither has a real call - which is the one place Firefox's
H.264 could turn from a footnote into the deciding factor. `tools/login.ps1` is there for it.
