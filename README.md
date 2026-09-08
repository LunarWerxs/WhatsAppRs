# whatsapp-rs

WhatsApp Web as a small native tray app for Windows, on a Chromium it ships itself. Written from
scratch, no forked code, MIT licensed. Not made by or affiliated with WhatsApp or Meta;
"WhatsApp" is their trademark and the page it shows is theirs.

**0.9 MB binary, ~350 MB of bundled engine beside it, 5 processes, 490 MB of RAM logged out.**
Most of that is WhatsApp's own JavaScript, not this wrapper. See "Why it is 500 MB" below,
because that is the question everybody asks and it has a measured answer.

## Download and run

1. Get `whatsapp-rs-<version>-windows-x64.zip` from the
   [latest release](https://github.com/LunarWerxs/WhatsAppRs/releases/latest).
2. Unzip it anywhere (a folder under `%LOCALAPPDATA%\Programs` is the usual place). There is no
   installer: the folder is the install.
3. Run `whatsapp.exe`, scan the QR code with your phone, done. Closing the window hides it to
   the tray; right-click the tray icon to quit.

Windows 10 or 11, 64-bit. The first run writes one Start Menu shortcut, `WhatsApp Rs`, because
Windows will not show toast notifications for a program that has none. Your login and message
cache live in `%LOCALAPPDATA%\WhatsAppRs`. To uninstall, delete the unzipped folder, that
directory, and the shortcut (plus the one in your Startup folder if you turned "Start with
Windows" on). Nothing else is touched.

**Quit from the tray menu, not from Task Manager.** A clean quit lets Chromium flush its cookies;
a kill can lose the login and cost you another QR scan.

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
- **Hardware accelerated**, and it stays that way: the process trim below was checked against the
  page's own WebGL adapter string so it could not silently fall back to software rendering.
- `whatsapp.exe --minimized` starts in the tray; `whatsapp.exe --quit` asks a running instance
  to shut down cleanly (useful from scripts).

## Privacy

The app talks to `web.whatsapp.com` and WhatsApp's own CDN, and to nothing else. It has no
telemetry, no update check and no crash reporting. Chromium's own background services (sync,
component updates, domain reliability reporting, translate, media router, optimisation hints,
autofill server calls) are switched off on the command line; the switch list is in
`src/cef_view.rs` and each one was measured before it stayed. Settings are two lines of plain
text in the data directory.

## Measured, 2026-09-07

Same instruments for every row: whole process tree, working set and private bytes, sampled after
the page reports itself ready. Method and the full sweep in FINDINGS.md.

| | processes | RAM | private | on disk | profile |
| --- | --- | --- | --- | --- | --- |
| **this app** (logged out) | **5** | **490 MB** | 347 MB | 310 MB | 36 MB |
| Meta's WhatsApp for Windows (logged in) | 8 | 1110 MB | 801 MB | 386 MB | 254 MB |
| plain Chrome `--app` (what the old C# wrapper drove) | 10 | 800 MB | 571 MB | installed | 102 MB |
| the old C# wrapper, measured 2026-09-06 | 10 | 803 MB | - | installed | 343 MB |

The last two rows agree to within 3 MB, measured a day apart by different scripts, which is the
check that the method is sound rather than flattering. Logged in with a real account and a full
chat list, the owner saw about 890 MB on his machine (2026-09-08), which is the same ratio to the
logged-out number that Meta's own app shows; a real mailbox is most of the cost.

**Meta's own app is a WebView2 shell** - `WebView2Loader.dll` sits in its install directory - so
it is Chromium too, and it uses more than twice the memory of this one.

## Why it is 500 MB, and why that is not fixable here

WhatsApp Web's own JavaScript heap is 61-72 MB used and 97-103 MB allocated **on a logged-out
login screen**, from ~25 MB of decoded script. Rendering that needs a modern browser engine, and
in 2026 the only engines that render `web.whatsapp.com` are a Chromium or a WebKit. Four
alternatives were built and measured before this one was chosen:

| | processes | RAM | verdict |
| --- | --- | --- | --- |
| the OS webview (Edge's engine, bundles nothing) | 3 | 372 MB | rejected: the phone lists it as "Microsoft Edge" |
| **bundled Chromium, this app** | 5 | 490 MB | shipped |
| bundled Firefox, driven as a separate process | 10 | 1115 MB | rejected: 15x the CPU, cannot be embedded |
| embedded Servo | 1 | ~1200 MB | rejected: ~30 fps, no WebRTC ever |
| native protocol client, no browser at all | 1 | **20 MB** | rejected: permanent ban risk |

That last row is the only genuinely light option and it is the one nobody can use: the protocol
is known only from reverse-engineering WhatsApp's apps, which their Terms forbid verbatim, and
accounts have been permanently banned for it. The choice is 20 MB with a ban risk or ~490 MB
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
tools\bundle.ps1 -KeepFallbacks   # assemble the shippable folder and report its size
tools\login.ps1          # open it on the persistent profile to scan a QR
```

Needs a Rust toolchain, Visual Studio Build Tools with the Windows SDK (`cmake`, `ninja`,
`rc.exe`), and about 1 GB of disk for the engine. The first build downloads the 171 MB CEF binary
distribution into `%USERPROFILE%\.local\share\cef`. The release zip is exactly what `bundle.ps1
-KeepFallbacks` produces: the required CEF files, one locale, and the software-rendering fallback
for machines without a working GPU driver.

## Layout

| file | role |
| --- | --- |
| `src/main.rs` | entry; hands CEF's subprocesses back to CEF, handles `--quit`, shows fatal errors in a message box |
| `src/cef_view.rs` | the whole app: window, engine, permissions, the notification bridge and mute shim, tray wiring |
| `src/notify.rs` | raising a Windows toast, and the three separate things that must be true first |
| `src/shortcut.rs` | the Start Menu shortcut carrying the AppUserModelID, and the Startup-folder one behind "Start with Windows" |
| `src/tray.rs` | tray icon and menu (embedded raw RGBA icons, no image crate) |
| `src/settings.rs` | the two menu toggles, persisted as `key=value` lines |
| `src/geometry.rs` | window placement, written only when it changes |
| `src/single_instance.rs` | loopback-port lock; also carries the `--quit` request |
| `src/paths.rs` | per-OS data directory |
| `tools/` | every instrument that produced a number in FINDINGS.md, with a README saying what each proves |

`FINDINGS.md` holds every measurement. `DECISIONS.md` holds the owner's rulings. Four other
engines and a native-protocol client used to live here and were deleted after being measured;
`git log` has them. `servo-patches/` is a by-product: the engine fixes that got WhatsApp Web
loading in Servo, kept as reference.
