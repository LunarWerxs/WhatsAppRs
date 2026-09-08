# whatsapp-rs

WhatsApp Web as a small native tray app, on a Chromium we ship. Written from scratch, no forked
code. Windows today; the design is portable but nothing else has been built.

**0.9 MB binary, 310 MB of bundled engine beside it, 5 processes, 490 MB of RAM.** Most of that
is WhatsApp's own JavaScript, not this wrapper. See "Why it is 500 MB" below, because that is the
question everybody asks and it has a measured answer.

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
check that the method is sound rather than flattering.

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

## What works, verified by running it

- **WhatsApp Web**, in our own window, reporting as Chrome 152.
- **Real Windows toasts**, headed "WhatsApp Rs", confirmed against Windows' own notification
  database and by screenshot. CEF displays no web notifications at all, so the app injects a
  shim that forwards them and raises the toast itself under a registered AppUserModelID.
- **Voice and video calls** have everything they need: WebRTC gathers ICE candidates, the
  microphone opens with its real device name, Opus is present. **H.264 is not** - see the gap
  below.
- **Close to tray, restore, single instance, clean quit**, all four PASS in `tools/tray-test.ps1`.
  `--quit` is the only correct way to stop it: Chromium flushes cookies on a clean shutdown and a
  kill loses the login.
- **Hardware accelerated**: the page reports `ANGLE (NVIDIA GeForce RTX 4070 Ti, Direct3D11)`.
- **Window geometry persists** and is only written when it changes.

## The one real gap

**No H.264.** The official CEF binaries are built without proprietary codecs and no switch
enables them. Measured: this build offers VP8, VP9 and AV1 and no H264. That costs a video call
that will not fall back to VP8, and it costs uploading an MP4 to WhatsApp (the page decodes it
locally first, and other CEF embedders have hit exactly this). Audio calls use Opus and are
unaffected. The only fix is building CEF from source with
`proprietary_codecs=true ffmpeg_branding=Chrome`, which is a full Chromium build.

Nobody has published what WhatsApp Web actually negotiates for video, so a real call from this
build is the test. `tools/login.ps1` sets that up.

## Build and run

```
tools\build.ps1          # cargo build --release, plus the cmake/ninja/MSVC the cef crate needs
tools\bundle.ps1         # assemble the trimmed shippable folder and report its size
tools\login.ps1          # open it on the persistent profile to scan a QR
```

The first build downloads the 171 MB CEF binary distribution into `%USERPROFILE%\.local\share\cef`.

## Layout

| file | role |
| --- | --- |
| `src/main.rs` | entry; hands CEF's subprocesses back to CEF, handles `--quit`, shows fatal errors in a message box |
| `src/cef_view.rs` | the whole app: window, engine, permissions, the notification bridge, tray wiring |
| `src/notify.rs` | raising a Windows toast, and the three separate things that must be true first |
| `src/shortcut.rs` | the Start Menu shortcut carrying the AppUserModelID, without which toasts are dropped |
| `src/tray.rs` | tray icon and menu (embedded raw RGBA icons, no image crate) |
| `src/geometry.rs` | window placement, written only when it changes |
| `src/single_instance.rs` | loopback-port lock; also carries the `--quit` request |
| `src/paths.rs` | per-OS data directory |
| `tools/` | every instrument that produced a number in FINDINGS.md, with a README saying what each proves |

`FINDINGS.md` holds every measurement. `DECISIONS.md` holds the owner's rulings. Four other
engines and a native-protocol client used to live here and were deleted after being measured;
`git log` has them.
