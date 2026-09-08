# Owner rulings and constraints

Settled decisions. Do not re-litigate these, do not ask for re-confirmation, do not propose
alternatives that contradict them. Owner is Michael.

## 2026-09-06

**1. Meta's official WhatsApp for Windows is REJECTED, permanently.**
His words: a ridiculous monstrosity that takes hundreds of megs of memory, hundreds of megs of
storage, and he hates it. Measured on his machine: 386.8 MB installed.
Consequence: never recommend it as an alternative, and never cite "the official app does it this
way" as a justification for an architecture choice. If research surfaces the fact that Meta's own
Windows app is itself a WebView2 shell around web.whatsapp.com, that is a neutral technical
data point and explicitly NOT an endorsement of matching it.

**2. The build must be SMALL and SIMPLE.** He expects it to be a small build because the app is
simple. That is the whole point of the exercise, and it is the tiebreaker on every stack choice:
when two options both work, take the smaller and simpler one.

**3. Cross-platform is a requirement: Windows, macOS and Linux.** Stated as "ideally", so it is a
strong preference rather than a hard gate, and Windows remains the primary target. But the
architecture must not paint Mac and Linux into a corner. This is what rules out a WebView2-only
design.

**4. Rust is the chosen language.** His call, not up for debate.

**5. The app embeds a webview rather than driving an external browser.** No shelling out to the
user's installed Chrome, which is what the C# original did.

**6. BUILD FROM SCRATCH. Do not use whatRust's code.** His words: he was building this himself, and
he does not want to use that author's code. We may **learn from its mistakes and its issue tracker**,
which is genuinely valuable (its four-month-old notification bug is a bug we have already solved), but
no code, no fork, no vendoring. Original implementation only.

**7. He dislikes Microsoft Edge and does not want the app to depend on it.** The honest technical
position, recorded so it is not re-argued each time:

- Cross-platform is NOT compromised by WebView2. wry uses the OS's own webview per platform, so
  Windows gets WebView2, macOS gets WKWebView, Linux gets WebKitGTK. **There is no Edge component on
  macOS or Linux at all.** Verified by running the probe on Linux against WebKitGTK 2.50.6.
- On WINDOWS specifically there is no small non-Edge option. Windows ships no system WebKit, and no
  non-Chromium/non-WebKit engine renders web.whatsapp.com. The only alternative is bundling Chromium,
  which Microsoft documents as adding over 250 MB to the installer, which contradicts ruling 2 and is
  the exact bloat he rejected in ruling 1.
- WebView2 is the engine WITHOUT the browser: no Edge UI, no Edge profile, no Bing, no Copilot
  sidebar, and it never launches Edge. Measured, it is lighter than the Chrome the current app already
  uses: 623.8 MB vs 802.9 MB RAM, 36.1 MB vs 102.9 MB fresh profile.
- Conclusion: WebView2 on Windows stands, on the grounds that it strictly reduces his current
  Chromium exposure. If he later rules against it anyway, that is his call, and the consequence is
  either a 250 MB bundled engine or no Windows build.

## 2026-09-07

**8. The engine is Servo, and the gaps get fixed by us and sent upstream.** His words: "use the
Servo, build the cache storage bullshit, run all of the tests, then do a pull request", and keep
what we add "an easy bolt-on instead of an integrated mess" so it survives their future changes.
Done as four commits on top of Servo `main`, each in Servo's own files and idiom, exported as
patches in `servo-patches/` with the PR text beside them. We run our own build of that branch
until upstream merges.

**9. No URL bar, no tab bar, nothing browser-shaped in the final product.** His words on seeing
the Servo window: "it's showing a URL input bar and a tab bar. It shouldn't show either of those."
That chrome belongs to `servoshell`, Servo's demo browser, which has no switch to turn it off.
Consequence: the product does not ship `servoshell`. It embeds the Servo engine in our own window
through Servo's embedding API, the same shape as the current app's use of WebView2, so there is no
toolbar to hide in the first place.

**10. Memory: 500 MB is still too much for him.** Measured on 2026-09-07: an empty page in Servo is
174 MB, WhatsApp on Servo is 545 MB by default, 492 MB with one layout thread and small thread
pools, 464 MB with the JS JIT disabled. A 128 MB JS heap cap crashes it, because WhatsApp's own
heap is bigger than that. Anything below the mid-400s means memory work inside Servo itself
(renderer and JS engine), which is real but is not a bolt-on. Recorded so the number is argued from
data, not feel.

**11. Light mode's window must look like WhatsApp, or light mode goes.** His words on seeing the
first chat window (bare Win32 list box, edit boxes, a Send button): "looks like shit", "something
out of a horror show", and: "if it's gonna look like that and we can't have a better UI, then we'll
probably have to call off the light mode." Target: "a literal clone of WhatsApp, where possible."
He doubts basic styling costs meaningful memory, and the mechanism agrees: drawing the chat rows
and message bubbles ourselves with GDI is the same machinery the bare controls already use; it is
GPU toolkits that cost tens of MB. The number is measured once the styled window exists, not
assumed. The functional window (19 MB working set, `--light-demo` to see it without linking) stays
as the base to restyle; the bare look ships nowhere.

Restyled the same day: WhatsApp Web's layout and palette (light and dark, following the Windows
setting, `WHATSAPP_RS_THEME=light|dark` to force one), drawn by the app with GDI and GDI+: avatars
with initials or the silhouette, chat rows with preview, time and unread badge, the search pill,
the chat header, date pills, bubbles, the composer with the green send button, dark title bar, the
app icon. Measured after: **20.2 MB working set, 3.0 MB private**, against 19.1 MB before the
restyle. His doubt was right; the styling cost one megabyte. "Dark mode is important" (his words
the same day) is why the theme follows Windows and why both were screenshotted.

**12. Safe mode's engine must be the same engine on every OS, and it must be ours to run:
Servo, embedded. No Edge, no Chrome, no Safari identity.** Said twice today, the second time in
anger, after safe mode showed up on his phone as "Microsoft Edge": the OS webview design (#7) means
Edge's engine on Windows, Safari's on Mac and Linux, and the phone names the device after whichever
it sees. He rejected the shortcut of presenting the OS engine as Chrome ("I don't want it using a
Google..."), and restated the requirement: an open-source engine from GitHub that we build and run
ourselves, presenting as something on WhatsApp's allowed list. That is #8, which was ruled yesterday
and never carried into the app: the Servo patches got built, safe mode stayed on WebView2.
Consequence, in work now: `src/servo_view.rs` embeds our Servo build behind `--features servo`
(`tools/build-servo.ps1`), presenting as Firefox, identical on Windows, macOS and Linux. The
WebView2 path stays only as the non-`servo` build until Servo mode is proven, then it goes.
Measured today, embedded: the app's own window with Servo inside draws WhatsApp's real login
page with a live QR, presenting as Firefox 143, one process, 442 MB working set (FINDINGS.md,
"Safe mode on Servo, embedded"). What it still needs is his phone: link it, and the device list
should read Firefox, not Edge. The cost he accepted in #10 stands: about 440 to 465 MB against
375 MB for the OS webview.

**13. Servo is retired as the engine (2026-09-07).** His words: "Servo has been, like, shitty."
It works: our fork logs in, renders the real chat list, and shows as Firefox on his phone, which was
the whole point of #12. It is also the slowest and heaviest option we have: about 30 frames per
second with 300 ms stalls, ~1.2 GB on a synced account against 803 MB for the Chrome wrapper it was
meant to replace, and no voice or video calls ever, because Servo has no WebRTC. Five engine patches
were needed before WhatsApp would load at all. Consequence: stop building on Servo. The fork, the
nine upstreamable patches and every measurement stay in the tree; nothing is deleted unless he asks.

**14. Light mode is retired (2026-09-07).** His words: "we'll ditch the WhatsApp, like, Rust bannable
engine as well, 'cause I don't really care about that." This is the native-protocol client from #8's
era: ~20 MB, and a permanent ban risk (see the ban rule at the top of HANDOFF.md). He was never going
to run it on his own number and would not get a spare one, so it existed only for other people.
Consequence: the code and its WhatsApp-styled window stay, unmaintained; the first-run picker that
chose between safe and light mode loses its reason to exist.

**15. The new direction: bundle an engine, and compare two (2026-09-07).** Build safe mode twice,
once on Chromium and once on Firefox, both trimmed down, and measure them head to head on memory and
responsiveness before choosing. He is not contradicting his dislike of those browsers: the objection
was ever having to run *someone else's installed browser*, or being labelled Microsoft Edge on his
phone. A bundled engine we control is a different thing. The asymmetry to design around, recorded so
it is not rediscovered: Chromium can be genuinely embedded (the `cef` crate, 152.0.0, published
2026-09-07), so we keep our own window, tray and close-to-tray; Firefox cannot be embedded at all
since Mozilla ended its embedding project, so a Firefox build means driving a separate process and
inheriting every problem the C# original's ChromeFinder, WindowFinder and Hooks classes existed to
solve.

## Standing consequence of 1 + 2 + 3

The honest tension to keep visible: there is no embeddable engine in 2026 that renders
web.whatsapp.com except a Chromium or a WebKit. So "small" applies to OUR binary and OUR profile
directory, and cannot apply to the rendering engine, which is either the OS's (free, already
installed) or bundled (large). Every footprint claim must say which of those it is counting, or it
is dishonest by omission.

## Measured baseline to beat (Windows, 2026-09-06)

The C# Chrome wrapper it replaces: 802.9 MB RAM across 10 processes, 342.8 MB profile directory.
The Rust + WebView2 probe: 623.8 MB RAM across 7 processes, 36.1 MB fresh profile,
projected about 92 MB once his real message history is present.

## Built and measured under #15, 2026-09-07

Not a new ruling: the record of what the two candidates turned out to be, so #15 is decided
from data. Full numbers and method in FINDINGS.md, "Bundled Chromium vs bundled Firefox".

**Both were built and both work**, in the app's own window, with the tray, close-to-tray,
single instance and window geometry the current design already has.

- **Chromium is embedded** (`src/cef_view.rs`, the `cef` crate 152.0.0+152.0.5). The browser is
  a child window of ours, exactly as WebView2 was, so nothing about the surrounding app
  changed. The phone will call it **Chrome**.
- **Firefox is adopted** (`src/firefox_view.rs`). Gecko cannot be embedded, so the app ships a
  real Firefox 155.0.1, launches it on a private profile, finds its window and takes it with
  `SetParent`. That works and holds. It also brings back three things this design had deleted:
  finding another process's window, keeping two processes alive and dead together (a job
  object), and forwarding keyboard focus by hand. The phone will call it **Firefox**.

Three things were learned that change the shape of the choice, and none of them was
predictable from documentation:

1. **The bundled Chromium has no H.264.** The official CEF binaries are built without
   proprietary codecs, and there is no switch that turns them on - it is compiled out.
   Measured: Firefox offers H264, our CEF offers VP8/VP9/AV1 only. Both have Opus, so audio
   calls are equal. What this costs is a video call that insists on H.264, and, separately and
   already documented by other CEF users, **uploading an MP4 to WhatsApp fails without it**.
   The fix exists and is expensive: build CEF from source with
   `proprietary_codecs=true ffmpeg_branding=Chrome`, which is a full Chromium build.
2. **CEF's Chrome runtime style crashes with a window we own.** It is the style that carries
   Chromium's own notification machinery, and with a browser created as a child of a native
   window it dies with an access violation immediately after creation, twice out of two, with
   nothing in the log. The build uses Alloy style, whose cost is that CEF then displays no web
   notifications at all - so the host draws them, which is exactly what the WebView2 build
   already had to do.
3. **Firefox's permission prompts are unreachable in an app window.** Firefox asks for the
   microphone with a doorhanger anchored to the URL bar, and the URL bar is what makes it look
   like a browser, so it is hidden. Measured: the request never gets an answer and a call would
   hang silently. Fixed by pre-granting in the profile the app writes.

Both bundles are big and within 20 MB of each other: **325 MB** for the trimmed Chromium,
**344 MB** for the trimmed Firefox, against about 2 MB for the OS-webview build that bundles
nothing. That is the price of the engine being ours rather than the machine's, and it is not
avoidable by trimming.

### The numbers #15 produced, 2026-09-07

Three runs each, logged out, whole process tree, sampled 60 s and 240 s after the page reported
itself ready. Method and every other configuration in FINDINGS.md.

| | processes | RAM at 4 min | private | CPU over 4 min | engine on disk |
| --- | --- | --- | --- | --- | --- |
| **bundled Chromium, one process** | **1** | **352 MB** | 286 MB | **9 s** | 325 MB |
| bundled Chromium, default | 7 | 560 MB | 376 MB | 11 s | 325 MB |
| bundled Firefox, trimmed | 10 | 1115 MB | 1040 MB | **149 s** | 344 MB |
| *control:* the OS webview (what safe mode runs today) | 3 | 372 MB | 198 MB | 11 s | nothing |
| *control:* plain Chrome (what the C# app drives) | 10 | 800 MB | 571 MB | 12 s | installed |

The last row reproduces the 802.9 MB measured for the C# wrapper on 2026-09-06 by a different
script on a different day, to within 2.5 MB, which is the check that the method is sound.

**Chromium wins on memory, on processor, on process count and on architecture.** Firefox has a
floor around 1.1 GB that six pref configurations could not move, and it spends about six tenths
of a core continuously on an idle page. Both engines pass close-to-tray, restore and clean quit;
both render WhatsApp Web correctly from a trimmed shippable bundle.

**Two things remain the owner's call**, and both are recorded here rather than decided:

1. **H.264.** Firefox has it, the public CEF binaries do not, and no switch enables it. It costs
   a WhatsApp video call that will not negotiate VP8 and it costs uploading an MP4. Fixable only
   by building CEF from source with proprietary codecs, which is a full Chromium build.
2. **`--single-process`.** It is what buys 352 MB instead of 560. Chromium does not support it
   and a renderer crash takes the app down. Nothing measurable breaks, and the WebView2 build
   already made the same trade.
