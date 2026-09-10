# whatsapp-rs

WhatsApp Web as a small native tray app for Windows, on a Chromium it ships itself. Written from
scratch, no forked code, MIT licensed. Not made by or affiliated with WhatsApp or Meta;
"WhatsApp" is their trademark and the page it shows is theirs.

[![Discord](https://img.shields.io/badge/Discord-join_the_community-5865F2?logo=discord&logoColor=white)](https://discord.gg/PsWpeNUzhk)

**One executable, no installer, nothing to unzip.** 130 MB to download, 6 processes, 518 MB of
RAM logged out. Most of that is WhatsApp's own JavaScript, not this wrapper. See "Why it is
500 MB" below, because that is the question everybody asks and it has a measured answer.

## Download and run

1. Download `whatsapp-rs-<version>-windows-x64.exe` from the
   [latest release](https://github.com/LunarWerxs/WhatsAppRs/releases/latest).
2. Run it. Scan the QR code with your phone. Done.

That is the whole thing. Put the exe wherever you like; nothing has to sit beside it. Closing the
window hides it to the tray; right-click the tray icon to quit.

**What the first run does, in about four seconds behind a small "Setting up" window:** it unpacks
the browser engine it is carrying into `%LOCALAPPDATA%\WhatsAppRs\engine\<version>-<id>`, where
the id is a short hash of the engine so a rebuilt version cannot be confused with an old one. That
folder
is ~350 MB and it stays there. **The single file is the download and the click; it is not a
smaller program.** Disk after first run and memory while running are exactly what v0.1.0's
17-file folder cost, because it is the same engine - Chromium exists only as a 271 MB DLL and
there is no static build of it, so no wrapper can be genuinely smaller. Meta does the same thing
inside an MSIX. A later version unpacks its own folder and deletes the old one.

Windows 10 or 11, 64-bit. The first run also writes one Start Menu shortcut, `WhatsApp Rs`,
because Windows will not show toast notifications for a program that has none. Your login and
message cache live in `%LOCALAPPDATA%\WhatsAppRs`. To uninstall, delete the exe, that directory
(the engine is inside it), and the shortcut - plus the one in your Startup folder if you turned
"Start with Windows" on. Nothing else is touched.

**Quit from the tray menu, not from Task Manager.** A clean quit lets Chromium flush its cookies;
a kill can lose the login and cost you another QR scan.

### There is no installer, and there is a portable mode

**The download IS the program.** No setup wizard, no Program Files, no registry keys, no uninstall
entry. Put the exe anywhere and run it. What it writes outside its own folder is exactly two
things: the engine and your profile under `%LOCALAPPDATA%\WhatsAppRs` (about 390 MB together),
and one Start Menu shortcut.

**To keep even those in one folder** - a USB stick, a synced folder, a machine you would rather
not leave anything on - put `tools/whatsapp-portable.cmd` next to the exe and run that instead.
It points the engine and the profile at subfolders beside itself, and uses a different
single-instance port so it does not collide with an installed copy. The whole thing then weighs
about **506 MB**: 124 MB exe, 346 MB unpacked engine, 36 MB profile. Copy the folder and your
login travels with it.

The one thing portable mode still leaves behind is that **Start Menu shortcut**, and it is not
laziness: Windows silently refuses to draw a toast for an application whose AppUserModelID it
does not know, and the shortcut is what carries that id. Delete it afterwards if you want no
trace at all, and accept that notifications stop working.

**The exe is not code-signed** (`Get-AuthenticodeSignature` says `NotSigned`), so Windows
SmartScreen shows "Windows protected your PC" the first time: More info, then Run anyway. A
certificate costs money and is on the list; until then, the SHA-256 is in the release notes and
`Get-FileHash` will tell you the download is the file that was published.

## What it does

- **WhatsApp Web** in its own window, reporting as Chrome. Your phone lists it as a browser
  session, which is what it is.
- **Real Windows toasts** for new messages, headed "WhatsApp Rs". The bundled engine displays no
  web notifications at all, so the app forwards them and raises the toast itself.
- **Voice and video calls**: the engine has WebRTC and Opus, and the microphone and camera are
  granted to WhatsApp without a prompt. See the H.264 note below.
- **Tray menu**: Open, Reload, **Mute sounds** (the message chime and the other alert tones go
  quiet, toasts still appear, voice messages and calls are untouched), **Show notifications**
  (toasts on or off), **Start with Windows** (a shortcut in your Startup folder that launches
  minimized to the tray), About, Quit.
- **Close to tray, single instance, remembered window position.** A second launch just brings
  the first one forward.
- **Hardware accelerated**, and it survives losing the GPU. The process trim below was checked
  against the page's own WebGL adapter string so it could not silently fall back to software
  rendering, and the GPU runs in **its own process**, so that when a display driver resets - which
  Windows does routinely - Chromium restarts it and falls back to software after a few tries. That
  is not a theoretical worry: an earlier build saved one process by putting the GPU inside this
  one, which silently removed that recovery, and a driver reset then cost a 244 GB log file and
  10.9 GB of RAM before anyone noticed. FINDINGS.md has the whole thing.
- **It watches itself.** The engine's debug log is capped at 16 MB, because Chromium neither
  rotates nor caps its own; and if the app ever catches itself writing megabytes a second or
  holding gigabytes of memory it raises a notification rather than quietly filling your disk.
- `whatsapp.exe --minimized` starts in the tray; `whatsapp.exe --quit` asks a running instance
  to shut down cleanly (useful from scripts).

## Privacy

The app talks to `web.whatsapp.com` and WhatsApp's own CDN, and to nothing else. It has no
telemetry, no update check and no crash reporting. Chromium's own background services (sync,
component updates, domain reliability reporting, translate, media router, optimisation hints,
autofill server calls) are switched off on the command line; the switch list is in
`src/cef_view.rs` and each one was measured before it stayed. Settings are two lines of plain
text in the data directory.

## Measured

Same instruments for every row: whole process tree, working set and private bytes, sampled 240 s
after the page reports itself ready, three runs, median. Method and the full sweep in FINDINGS.md.
This app's row was re-measured on 2026-09-09 after the GPU moved back into its own process; the
other three are from 2026-09-07 and are unaffected by that.

| | processes | RAM | private | on disk | profile |
| --- | --- | --- | --- | --- | --- |
| **this app** (logged out) | **6** | **518 MB** | 350 MB | 347 MB | 36 MB |
| Meta's WhatsApp for Windows (logged in) | 8 | 1110 MB | 801 MB | 386 MB | 254 MB |
| plain Chrome `--app` (what the old C# wrapper drove) | 10 | 800 MB | 571 MB | installed | 102 MB |
| the old C# wrapper, measured 2026-09-06 | 10 | 803 MB | - | installed | 343 MB |

The last two rows agree to within 3 MB, measured a day apart by different scripts, which is the
check that the method is sound rather than flattering. Logged in with a real account and a full
chat list, the owner saw about 890 MB on his machine (2026-09-08), which is the same ratio to the
logged-out number that Meta's own app shows; a real mailbox is most of the cost.

**Meta's own app is a WebView2 shell** - `WebView2Loader.dll` sits in its install directory - so
it is Chromium too, and it uses more than twice the memory of this one.

**It went up by about 28 MB and one process on 2026-09-09, deliberately.** The previous build
ran Chromium's GPU inside the browser process to save exactly that, which also removed
Chromium's ability to notice the GPU had died and fall back to software - and a routine driver
reset then ran away for nine hours. Private bytes are unchanged (350 MB against 347) and frame
timing is identical; the whole difference is one more process. DECISIONS.md #24.

## Why it is 500 MB, and why that is not fixable here

WhatsApp Web's own JavaScript heap is 61-72 MB used and 97-103 MB allocated **on a logged-out
login screen**, from ~25 MB of decoded script. Rendering that needs a modern browser engine, and
in 2026 the only engines that render `web.whatsapp.com` are a Chromium or a WebKit. Four
alternatives were built and measured before this one was chosen:

| | processes | RAM | verdict |
| --- | --- | --- | --- |
| the OS webview (Edge's engine, bundles nothing) | 3 | 372 MB | rejected: the phone lists it as "Microsoft Edge" |
| **bundled Chromium, this app** | 6 | 518 MB | shipped |
| bundled Firefox, driven as a separate process | 10 | 1115 MB | rejected: 15x the CPU, cannot be embedded |
| embedded Servo | 1 | ~1200 MB | rejected: ~30 fps, no WebRTC ever |
| native protocol client, no browser at all | 1 | **20 MB** | rejected: permanent ban risk |

That last row is the only genuinely light option and it is the one nobody can use: the protocol
is known only from reverse-engineering WhatsApp's apps, which their Terms forbid verbatim, and
accounts have been permanently banned for it. The choice is 20 MB with a ban risk or ~520 MB
without one. There is nothing in between, and Meta ships the same thing at 1110 MB.

## The one real gap

**No H.264.** The official CEF binaries are built without proprietary codecs and no switch
enables them. Measured: this build offers VP8, VP9 and AV1 and no H264. That may cost a video
call that will not fall back to VP8, and it costs uploading an MP4 to WhatsApp (the page decodes
it locally first, and other CEF embedders have hit exactly this). Audio calls use Opus and are
unaffected. The only fix is building CEF from source with
`proprietary_codecs=true ffmpeg_branding=Chrome`, which is a full Chromium build.

Nobody has published what WhatsApp Web actually negotiates for video, so a real call from this
build is the test. If you make one, open an issue with the result either way.

## Build from source

```
tools\build.ps1          # cargo build --release, plus the cmake/ninja/MSVC the cef crate needs
tools\single-exe.ps1     # bundle, compress, append -> the one file the release ships
tools\single-test.ps1    # prove it: that file alone, in an empty folder, started twice
tools\login.ps1          # open it on the persistent profile to scan a QR
```

Needs a Rust toolchain, Visual Studio Build Tools with the Windows SDK (`cmake`, `ninja`,
`rc.exe`), Python 3 with `zstandard`, and about 1 GB of disk for the engine. The first build
downloads the 171 MB CEF binary distribution into `%USERPROFILE%\.local\share\cef`.

A build straight out of `target\release` runs from there with the engine beside it, which is what
every test script uses and what the development loop depends on. `single-exe.ps1` then takes what
`bundle.ps1` assembles - the required CEF files, one locale, and the software-rendering
fallback, which is what Chromium uses both on a machine with no working GPU driver and after a
driver reset takes the real one away - compresses the sixteen
files that are not the exe into one zstd frame, and writes the exe, that frame and a 64-byte
footer as a single file. Windows ignores bytes past the end of a PE image, so the result is an
ordinary executable. `libcef.dll` is delay-loaded, which is what lets it start with nothing
beside it at all.

## Layout

| file | role |
| --- | --- |
| `src/main.rs` | entry; finds or unpacks the engine, hands CEF's subprocesses back to CEF, handles `--quit`, shows fatal errors in a message box |
| `src/engine.rs` | the engine the exe carries: the footer, the container format, unpack-and-verify, loading the DLLs, deleting old versions |
| `src/setup_window.rs` | the small "Setting up" window with its progress bar, on its own thread, shown only while unpacking |
| `src/cef_view.rs` | the whole app: window, engine, permissions, the notification bridge and mute shim, tray wiring |
| `src/notify.rs` | raising a Windows toast, and the three separate things that must be true first |
| `src/shortcut.rs` | the Start Menu shortcut carrying the AppUserModelID, and the Startup-folder one behind "Start with Windows" |
| `src/tray.rs` | tray icon and menu (embedded raw RGBA icons, no image crate) |
| `src/settings.rs` | the two menu toggles, persisted as `key=value` lines |
| `src/geometry.rs` | window placement, written only when it changes |
| `src/single_instance.rs` | loopback-port lock; also carries the `--quit` request |
| `src/watchdog.rs` | the app watching itself: caps the engine's log, and says so if it catches itself running away |
| `src/paths.rs` | per-OS data directory |
| `tools/` | every instrument that produced a number in FINDINGS.md, with a README saying what each proves |

`FINDINGS.md` holds every measurement. `DECISIONS.md` holds the owner's rulings. Four other
engines and a native-protocol client used to live here and were deleted after being measured;
`git log` has them. `servo-patches/` is a by-product: the engine fixes that got WhatsApp Web
loading in Servo, kept as reference.
