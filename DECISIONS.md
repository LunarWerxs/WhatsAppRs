# Owner rulings and constraints

Settled decisions. Do not re-litigate these, do not ask for re-confirmation, do not propose
alternatives that contradict them. Owner is Michael.

---

# ⛔⛔ READ THIS BEFORE PROPOSING ANY ENGINE. EVER. ⛔⛔

# THE ENGINE MUST BE OS-AGNOSTIC. THE SAME ENGINE ON WINDOWS, MACOS AND LINUX. NO EXCEPTIONS.

> **"I will never, ever, ever use a browser that is not OS agnostic."**
>
> Michael, 2026-09-09, after WebView2 was raised for the third time across this project.
> Paraphrased at his request; the original was considerably more emphatic and meant exactly
> this. Read the "never" four times if the force is not coming through.

**If it does not run on Windows AND macOS AND Linux, it is not a candidate. Do not raise it. Do
not raise it "just to note the option". Do not raise it because it would be smaller. Do not raise
it because someone on the internet did it. The size argument is dead: it has been searched to the
end and written up in this file, and the answer does not change the rule.**

**WebView2 / Microsoft Edge is PERMANENTLY OUT and is never to be mentioned as an option again.**
Microsoft ships WebView2 for **Windows only**. There is no Linux build. There is no macOS build.
It therefore fails this rule at the first hurdle, before anyone gets to the size, the user agent,
the identity, or how many megabytes it would save. Every "but the user agent can be spoofed now"
thread is answered by the line above, not by a counter-argument.

The same disqualification applies to **every** OS-supplied webview by definition: WKWebView on
macOS and WebKitGTK on Linux are DIFFERENT ENGINES, so "use the system webview" is three engines
wearing a trench coat, not one. That is the arrangement ruling #12 already rejected in anger.

This is why the app bundles CEF, and why 271 MB of libcef.dll is the accepted price. See #7, #12
and #26.

---

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
era: ~20 MB, and a permanent ban risk (the ban risk is spelled out in #19). He was never going
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

**16. The engine is a bundled Chromium, and everything else is deleted (2026-09-07).** His
words on the comparison: "Build your recommendation. Delete the rest, make things as light...
as reasonable." Done. `src/cef_view.rs` is the only engine; the OS webview, the embedded Servo,
the bundled Firefox and the native-protocol light mode were all built, measured against it, and
removed - twelve source files, sixteen instruments and a 345 MB bundled Firefox runtime. The
binary went from 17 MB to 0.9 MB because their dependencies went with them. Everything is in
`git log`; FINDINGS.md keeps the numbers that justified each removal.

**17. NO `--single-process` (2026-09-07).** His words: "No single process." It measured 352 MB
in one process against 490 MB in five, and it is the single biggest memory lever available, so
this is a deliberate purchase: about 150 MB to keep the renderer isolated, because Chromium does
not support single-process mode and a renderer crash there takes the whole app down instead of
showing an error page. The switch is not in the code; `WHATSAPP_RS_CEF_SWITCHES=single-process`
still reaches it for measurement.

Shipped configuration, chosen by a five-way sweep on 2026-09-07: `in-process-gpu`,
`process-per-site`, `renderer-process-limit=1`, on top of the feature-disabling switches.
**5 processes, 490 MB** (three runs, 483-504) against 550 MB and 7 processes stock. `in-process-gpu` and deliberately
not `disable-gpu`: disabling the GPU saved slightly more and dropped Chromium onto a software
rasteriser, which is the shape of the mistake the Servo round made. Verified after the change
that the page still reports the real adapter (`ANGLE (NVIDIA GeForce RTX 4070 Ti, Direct3D11)`)
and that every capability WhatsApp needs is still present.

**18. Meta's own app is not the lighter option, measured (2026-09-07).** The recurring question
"is the real app lighter?" now has a number. It is a WebView2 shell - `WebView2Loader.dll` sits
in its install directory - so it is Chromium too, and measured on this machine **logged in with
the real account: 8 processes, 1110 MB working set, 801 MB private, 386 MB on disk, 254 MB of
profile.** That is more than twice ours and it is the app he would otherwise be running. #1
stands, and now it stands on a measurement rather than on a feeling about bloat.

**19. There is no lighter WhatsApp to run, and the APK route leads back to light mode
(2026-09-07).** He asked whether the Android app could be taken apart and run instead. It
cannot, and the reason matters: an APK is Android bytecode plus native libraries written
against the Android framework, so "running what is inside it" means running Android. The part
that would actually be worth extracting - the protocol - has already been extracted by other
people, and that library is exactly what light mode was: about 20 MB, one process, and a
permanent ban risk, because pulling the protocol out of their app is the clause their Terms
forbid. It was built, measured, and retired in #14 for that reason. The choice has always been
those two things and nothing in between: speak their protocol at 20 MB with a ban risk, or
render their website at 350-560 MB with none.

**20. Public, MIT, with a build anyone can run (2026-09-08).** His words after running it on his
own account: "893 megs. I like it. Let's get this into a public repo or whatever, push a build so
other people can use it, and make sure the executable has an icon." The repository is
`LunarWerxs/WhatsAppRs`, public, MIT (the licence every other public LunarWerx repo uses), with
the trimmed bundle attached to a GitHub release as a zip. The public zip keeps the
software-rendering fallback (+37 MB on disk, no RAM cost) because strangers' machines have GPU
drivers ours does not. Everything from the multi-engine era that no longer described the code -
the handoff, the architecture memo, the Linux WebKitGTK probe - was deleted rather than shipped
stale; the Servo build notes moved beside the Servo patches they belong to.

**21. "Mute" means the sound and only the sound (2026-09-08).** His words: "at least a mute
notifications option. Specifically mutes, not, like, stop the notifications, just make it so
there's no sound." So the tray has two separate toggles: **Mute sounds** silences WhatsApp's alert
tones and leaves toasts, voice messages and calls alone; **Show notifications** is the other
thing, off by choice only. The engine's whole-page audio mute was rejected because it would have
silenced a call. Beyond those, the menu carries what a public build needs and nothing more:
Reload, Start with Windows, About, Quit.

**22. The Servo bug report goes in, under this project's name, disclosed as AI-written
(2026-09-08).** After being told their contributor guide bans LLM-written contributions: "Just
open an issue then if their no LLM policy is written, which is some bullshit... Feel free to let
them know that the issue was opened by an AI." The issue is a bug report, not code; it says at the
top what wrote it; the patches stay on the LunarWerxs fork as reference and are not offered as
pull requests. Their policy, their call; the disclosure is so they can apply it.

**23. v0.2.0 is one file (2026-09-08).** On seeing the 17-file zip: "that should all be, like,
combined into a single executable." It cannot be a static binary - CEF exists only as a 271 MB
DLL - so it is one exe carrying the engine as a compressed payload, unpacked once into
`%LOCALAPPDATA%\WhatsAppRs\engine\<version>` on first run, with `libcef.dll` delay-loaded so the
exe starts without it beside it. Changes the download and the click, not the disk or the RAM,
and the README must say so.

**Shipped 2026-09-08 as v0.2.0: one 129.8 MB file.** zstd -22 over xz -9e because it was
measured - 6 MB bigger, 7x faster to unpack, and the unpack happens on every stranger's first
run. Proved on a copy of only that file in an empty folder: it starts (delay-load), unpacks
345.9 MB in ~4 s behind a progress window, loads the page, and does not unpack again. tray-test
9/9, mute-check PASS, capability-probe unchanged, GPU still hardware. Memory 493.1 MB against
v0.1.0's 489.6 over three runs each, inside v0.1.0's own spread, so the payload does not stay
resident. FINDINGS.md #17 has the numbers and the traps.

## 2026-09-09

**24. `--in-process-gpu` is reversed. A memory saving that removes a circuit breaker is not a
saving.** This is the only entry here that undoes an earlier one, and it is worth reading as
what it is: the sweep in #17 was correct about every number it measured, and it measured the
wrong things. It asked what the switch cost in processes and megabytes. It did not ask what the
switch was load-bearing for.

What it was load-bearing for: `viz::GpuServiceImpl::MaybeExitOnContextLost` begins by checking
whether the GPU service is in the host process, and if it is, returns - the comment upstream is
that the GPU process cannot be restarted from inside itself, so it just hopes for recovery.
Out of process, that same function calls `RestartGpuProcessForContextLoss`, the GPU process
exits, `GpuProcessHost::RecordProcessCrash` counts it, and after about three the browser falls
back to SwiftShader for the rest of the session. In process there is no process to exit, no
counter, and no fallback. `--in-process-gpu` did not weaken Chromium's GPU recovery; it deleted
it.

On 2026-09-09 at 04:10:37 an NVIDIA driver reset (`nvlddmkm` event 153, the eighth in three
days on this machine) took the D3D11 device away. `eglCreateContext` was then retried about
seven thousand times a second for eight hours and forty-five minutes. `cef.log` reached
**243,629,516,234 bytes** growing at 8.54 MB/s, the browser process reached **10.9 GB** private
growing at 1.18 GB/hour, and one thread burned 8.3 CPU-hours. The system drive went to 7.4%
free, about two hours from full. It was found because the owner noticed the RAM figure, not
because anything in the app reported it.

**Re-measured properly afterwards, three runs at 240 s: 6 processes and 518 MB against the old
5 and 490.** One process and about 28 MB of working set. Private bytes unchanged - 350 MB against
347 - and frame timing identical at 120.2 fps. That is what it bought. It is gone, and it must not come back;
`WHATSAPP_RS_CEF_SWITCHES=in-process-gpu` still reaches it for a measurement and nothing else.
`process-per-site` and `renderer-process-limit=1` stay - neither has anything to do with this.
`--single-process` is doubly ruled out now: the owner's ruling in #17 stands, and it implies
`in-process-gpu`, so it carries this defect as well.

**Two things follow from it that are not the switch.** They matter more than the switch does,
because the switch was one mistake and these are the reasons a mistake ran for nine hours.

- **CEF's log had no bound and nothing watched it.** Chromium neither rotates nor caps, and the
  app pointed it at the user's system drive and never looked again. `src/watchdog.rs` now caps
  it at 16 MiB on the event loop's existing 1.5 s tick, keeps the first capful as
  `cef.log.onset` because the head of the log is where a root cause lives, and raises one toast
  the first time the app catches itself writing megabytes a second or holding gigabytes of
  private memory. It deliberately does **not** restart or quit: a messaging client that shuts
  itself down on a heuristic is worse than one running hot, and killing Chromium skips the
  cookie flush and costs the login.
- **Nothing here ran long or broke anything on purpose.** Every number in this repo came from a
  four-minute sample on a login page, so no instrument could have seen this. `tools/soak.ps1`
  is the regression test: fifteen checks, the first of which is simply that a
  `--type=gpu-process` child exists.
- **And nothing ran any of it automatically.** There was no CI in this repository at all, which
  is the reason a switch could sit in the shipped list for two days. `.github/workflows/ci.yml`
  now runs formatting, clippy and `cargo test` - which carries
  `in_process_gpu_is_never_shipped` - on every push, on a Windows runner, on a **pinned**
  toolchain so that a new clippy release cannot turn it red on its own. The soak is deliberately
  not in it: ten minutes and a real Chromium is a thing a person runs before shipping. The
  reasoning lives in the workflow's own comments, beside the thing it governs.

**And a third thing, found while fixing it: the shipped bundle did not carry what the fix falls
back TO.** `bundle.ps1` had a `-KeepFallbacks` switch, off by default, worth ~37 MB, whose
reasoning was "this machine has a GPU". Restoring the out-of-process GPU makes Chromium fall back
to `vk_swiftshader.dll` after about three context-loss failures - and with an empty fallback stack
Chromium stops the browser process instead. So the recovery this whole entry is about would have
ended in the app closing rather than in a stutter. The switch is **gone**; those files are always
included now. `single-exe.ps1` always passed `-KeepFallbacks`, so what actually shipped to people
was fine - but `bundle.ps1`'s default output folder is `D:\wa-bundle\whatsapp`, which is the
folder the owner's own install runs from, so one plain run of it would have quietly disarmed a
live app.

**25. The handoff notes stop being published, and history is NOT rewritten (2026-09-09).**
Delegated call, made on Michael's "do whatever else you recommend" after the options were put to
him. `NEXT_PROMPT.md` sat TRACKED at the repository root, so GitHub served it continuously from
the moment this repo went public: its own headings included "Still open, and both need his phone",
"Traps this cost, so nobody pays for them twice" and "How Michael wants you to work", plus four
machine paths and both brothers' first names. That is exactly the candid writing the standing rule
(2026-08-29, restated 2026-09-09: a public repo does not publish its to-do list) exists to keep
unpublished.

It has moved to `docs/todo/`, which is gitignored, so it stops being served going forward.

⛔ **History was deliberately NOT rewritten**, and the reasoning matters more than the act. Checked
first: the file contains **no credentials, no tokens, no email addresses and no phone numbers** -
it is candid, not sensitive. Rewriting the history of a public repository breaks every clone and
fork, and it does not retract anything anyway: git has already served it, and GitHub caches, forks
and archives keep their copies. Paying that cost to un-publish two first names and some `D:\` paths
would be theatre. If something genuinely secret ever does land here, the answer is different and
starts with rotating the secret, not with rewriting history.

**26. ⛔ THE ENGINE IS OS-AGNOSTIC OR IT IS NOT A CANDIDATE. WebView2 is permanently closed
(2026-09-09).** His words, paraphrased at his own request: *"I will never, ever, ever use a
browser that is not OS agnostic."* Said after WebView2 came up for the third time in this
project's life, the third time being mine. He was blunter than that and entitled to be.

**The rule, stated so there is nothing left to interpret:** an engine is only a candidate if the
SAME engine runs on Windows, macOS and Linux. Not "an engine on each". The same one. Anything
that fails that is out before its size, its speed, its identity or its convenience is discussed,
and it is out permanently rather than pending better arguments.

**WebView2 fails at the first hurdle: Microsoft ships it for Windows only.** No Linux build
exists. No macOS build exists. Every clever thing that is true about it downstream of that
(`CoreWebView2Settings.UserAgent` is settable, the client hints can be suppressed, a shipping
wrapper installs at 2 MB) is irrelevant, because the rule is not about the user agent and never
was. #7 and #12 said this already, in 2026-09-06 and 2026-09-07; this entry exists because the
option kept being re-raised anyway, including by me, on the grounds that new facts had appeared.
New facts about a disqualified candidate do not requalify it.

**By the same rule, "just use the system webview" is out on every platform, permanently.** It
means WebView2 on Windows, WKWebView on macOS and WebKitGTK on Linux: three different engines,
three different bug surfaces, three different identities on the phone's linked-device list. That
is precisely the arrangement #12 rejected.

**Do not raise this to ask whether the rule still holds.** It holds. The size question that
motivated every re-raise has now been searched to the end and answered: no smaller engine capable
of rendering web.whatsapp.com exists anywhere, CEF's own maintainer says the size is inherent to
Chromium, and every wrapper that bundles a real Chromium lands in our band. 271 MB is the price of
the rule, the price is accepted, and the matter is closed.

**27. 64-bit stays. 32-bit is rejected even though it measurably wins (2026-09-10).** His words
when it was put to him: *"I wasn't asking you to research 32-bit... I would prefer to stick with
64."* Recorded because the number is genuinely attractive and someone will find it again.

**The measurement is real and was made by downloading CEF's own windows32 minimal distribution
and counting bytes**, not estimated: `libcef.dll` is **227.1 MB on 32-bit against 271.4 MB on
64-bit, 44.3 MB and 16.3% smaller**; `dxcompiler.dll` (24.6 MB) is absent from the 32-bit
distribution entirely; `chrome_elf.dll` is 1.1 MB smaller. The data files (`resources.pak`,
`icudtl.dat`, `v8_context_snapshot.bin`) are byte-identical, which is the expected shape, since
32-bit wins on code and nothing on data. Total: about **70 MB off a 347 MB install, near 20%**.

**Rejected anyway, and the reasons are good ones.** A 32-bit process caps at 4 GB of address
space, which is a ceiling this app does not have today and which a long chat history plus a video
call is exactly the case to find. CEF's own maintainer calls 32-bit Windows "effectively a legacy
platform", so it is the target that gets dropped first. And the missing `dxcompiler.dll` is an
unproven capability question, not a clean saving.

**Do not re-propose 32-bit for size.** The number is known, it is 70 MB, and the answer is still
no. Size work continues on 64-bit only.


**28. The engine is compressed on disk, in place, on first run. 345.9 MB becomes 156.0 MB
(2026-09-10).** The owner asked the size question three times and refused two answers that came
from reasoning rather than measurement. This is what came back with a number attached, and it is
the only saving of the whole search that survives.

**What ships:** after the payload extracts into its versioned folder, `engine.rs` runs
`compact.exe /c /s:<dir> /exe:LZX /i /q` on that folder, once, before anything maps the files.
**190.0 MB saved, 54.9%.** `libcef.dll` alone goes 271.4 MB to 115.4 MB.

**It costs about +0.27 s of startup and no memory at all** - working set 566.6 MB compressed
against 567.4 MB uncompressed, measured with the page cache flushed and the compressed run going
FIRST so every cache advantage worked against it. That "no memory" result is the mechanism, not
luck: WOF decompresses into the SHARED file cache, so pages stay shared across all six processes.

**This is not UPX and the difference is the whole point.** UPX-packing `libcef.dll` is on record
on the CEF forum as crashing cefclient, because a packer decompresses the image eagerly into
private memory and destroys the memory-mapping the loader depends on. WOF leaves an ordinary PE
that the loader maps exactly as before.

**Best effort, always, and silence is the correct failure.** Not NTFS (a FAT32 stick, which
portable mode makes a real case), an older Windows, a group policy, a missing `compact.exe` - any
of those and the engine is simply left uncompressed, which is precisely what shipped before this
existed. `WHATSAPP_RS_NO_COMPRESS=1` turns it off for a measurement run.

**What was rejected and why:** XPRESS16K/8K/4K (191.9 / 200.0 / 214.4 MB against LZX's 156.0, with
the startup difference between modes inside the noise - 36 MB for nothing); and
`WofSetFileDataLocation`, because it needs per-file plumbing and a provider struct where this is
one documented call, and one call is what the numbers above were measured with.

**29. The search for a smaller browser engine is CLOSED, by measurement, not by opinion
(2026-09-10).** Filed so that the fourth person to ask - and there will be one - gets numbers
instead of another round of searching. The full working is FINDINGS.md, "The size question,
measured to the end and closed".

- **No smaller modern `libcef.dll` exists in public.** 80 GitHub repos, every release, every
  asset, 36 CEF binaries, 0 failed API calls. The one 50.9 MB candidate posted three days earlier
  is a **2015-era Chromium with zero occurrences of `WebAssembly`** in the binary, so it cannot
  render WhatsApp Web at all. Everything else is a repackage of the official distribution.
- **Stripping Google's services is worth 0.2 MB.** ungoogled-chromium's `chrome.dll` against
  stock Chrome's, same Chromium branch, x64: 284.0 against 284.3 MB. A project whose entire
  purpose is that removal moves the engine by one part in a thousand.
- **CEF does not forbid feature-stripping** - an earlier claim in this project that it did was
  wrong, and `tools/gn_args.py` says so plainly: `GN_DEFINES` is a documented pass-through and
  the only thing genuinely forced is DRM. The blocker is cost, not permission. Never restate it
  as permission.
- **One unmeasured lever remains: `optimize_for_size=true`**, upper bound ~48 MB from a 2016
  measurement that predates ThinLTO and PGO. Buying it means owning a from-source Chromium build
  forever, on every CEF bump. **Recommendation: no**, and it is the owner's call to reverse.

**271 MB is the price of the rule in #26, the price is accepted, and re-searching it is not
work.** If someone wants the last 48 MB, the path is a build project, not another search.

**30. The release WILL be code-signed, through Azure Artifact Signing, and NOT yet (2026-09-10).**
His words: "we will be... signing this at some point, as I already have a plan. But at this exact
moment, we will... Hold off. You may also... note that we will be using the Connections MCP
server, which has Azure signing in it."

Two separate rulings, and the second is the one that is easy to get wrong:

- **The route is decided: Azure Artifact Signing** (the service formerly called Trusted Signing),
  driven from this workspace's own tooling against a vaulted credential, with no signing key ever
  handled by a person and none in a repository or a CI job. That closes the "a certificate costs
  money and is on the list" line the README and the handoff notes had carried since v0.1.0. It
  was never the real blocker and must not be restated as one.
- **Nothing is signed now, and nothing is prepared for it either.** He is holding; lifting the
  hold is his call alone. Do not sign a release, do not provision a signing credential, and do
  not add a signing step to a release script or workflow "ready for when we turn it on" - a
  signing call that exists is a signing call that gets run by accident.

Two things signing will quietly break, recorded here so they are not discovered on release day:
the **published SHA-256**, because signing rewrites the file and a hash taken before it would
never match a stranger's download; and possibly the **engine payload**, which the exe finds by
seeking back from the end of its own file, while Authenticode appends a certificate table to the
end of the file too. Whether those collide is one five-minute test on a throwaway build, and it
happens before anything ships. The runbook is in this repository's (gitignored) to-do list.

What stays true meanwhile: the exe is unsigned, SmartScreen warns on it, and the SHA-256 in the
release notes is how a stranger checks the download. The README says exactly that, says signing
is planned, and promises no date.
