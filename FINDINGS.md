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
(`chats-loaded.png`). One process, about 1.0 GB working set with a fully synced account, which is
substantially more than the 442 MB measured on the logged-out screen and is worth its own look.

The instrument that made this tractable was forwarding the page's console and capturing unhandled
rejections **with their message**, not just a stack. The first attempt logged only stacks and named
nothing; adding `name: message` turned a day of guessing into two lines that said exactly what was
missing.

Still open after all this: **OPFS** is absent and cannot be shimmed cheaply, and one
`DataError: Provided data is inadequate.` rejection still appears during load without visibly
breaking anything. `IDBCursor.update`/`delete` remain unimplemented, and index queries scan the
whole object store, which will not scale to a large mailbox.

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
