# Paste this into a fresh chat

You are picking up a WhatsApp desktop app for Michael. Everything below was built and measured on
this machine (Windows 11, 32 cores). Numbers are measured, never estimated. Read
`D:\NEWProjects\WhatsAppRs\FINDINGS.md` and `DECISIONS.md` before changing direction on anything.

---

## The job

**Build two new versions of safe mode and measure them against each other: one on bundled Chromium,
one on Firefox.** Trimmed down and lightweight in both cases. Then Michael picks one.

Safe mode means: our own window and tray around `https://web.whatsapp.com`, which is an ordinary
browser visiting an ordinary website. It carries no account risk and never has.

## What is now dead, and why. Do not revive either without being asked

**Servo is retired (2026-09-07).** We embedded a Servo fork successfully. It logs in, loads the real
chat list, and presents as Firefox on the phone. It also runs at roughly 30 frames per second with
300 ms stalls, uses about 1.2 GB on a synced account, and can never do voice or video calls because
Servo has no WebRTC. Michael's verdict: "Servo has been, like, shitty." He is right. It took five
engine patches to make WhatsApp load at all, and the result is still the slowest option we have.

**Light mode is retired (2026-09-07).** That is the `whatsapp-rust` client that speaks WhatsApp's
protocol directly with no browser: about 20 MB, and a permanent ban risk because the protocol is
known only from reverse engineering, which their Terms forbid. Michael never wanted it on his own
number and has now dropped it entirely: "I don't really care about that."

Neither is deleted. The code, the measurements and nine upstreamable Servo patches are all still in
the tree, because throwing away work that was measured is worse than leaving it sitting there.
Deleting is a one-line ask if he wants it. **Do not spend time on either.**

## The measured baseline. Do not re-derive this

Same machine, same account where noted.

| what | memory | processes | notes |
| --- | --- | --- | --- |
| C# Chrome wrapper (what he ran before all this) | 803 MB | 10 | his real account; the number to beat |
| OS webview (WebView2, Edge's engine) | 375-425 MB | 3 | logged out only; never measured logged in |
| Servo, embedded | ~1.2 GB | 1 | his real account, synced |
| Servo, logged out | 442 MB | 1 | |
| light mode (retired) | 20 MB | 1 | not a browser at all, so not comparable |

Michael has said 500 MB is too much (DECISIONS.md #10) and rejected Meta's official app at 386 MB
for being bloated. Treat 803 MB as the number to beat and 500 MB as the target, while being honest
that WhatsApp Web's own JavaScript heap is 69-97 MB before any engine exists, so there is a floor.

## The two candidates, and the asymmetry that decides the design

**Chromium: embed it.** The `cef` crate (Chromium Embedded Framework bindings) is at 152.0.0,
published 2026-09-07, tracking Chromium 152, which is the same major the OS webview here runs. CEF
gives a real child window we own, so the tray, close-to-tray and single-instance code all keep
working unchanged. Expect 150-250 MB on disk for the bundled runtime. Start here: it is the
straightforward one.

**Firefox: you cannot embed it.** Gecko has had no supported embedding API since Mozilla ended the
embedding project, and GeckoView is Android only. So a "Firefox version" means shipping a real
Firefox and driving it as a separate process: a private profile directory, `--kiosk`, and a
`userChrome.css` to strip the remaining browser UI. **This drags back every problem the current
design deleted**: finding the window of another process, faking close-to-tray with global hooks,
and no clean way to own the window. The decompiled C# original at
`D:\NEWProjects\WhatsAppWebApp` did exactly this for Chrome, and its `ChromeFinder`, `WindowFinder`
and `Hooks` classes are what that costs. Read them before you promise Michael parity. Reparenting
the Firefox window into ours with `SetParent` is possible on Windows and worth one timeboxed
experiment, but say plainly that it is a hack.

If Firefox turns out to be structurally worse, say so early rather than burning a day proving it.

## Measure them like this, and beware your own numbers

Use `tools/jank-run.ps1` (frame timing) and the memory method already in FINDINGS.md: same account,
same window size, working set at one and four minutes after the chat list appears.

**Run every configuration at least three times.** The single most expensive mistake of the last
session was trusting single measurements: the same build measured 8.5 fps and 31.2 fps on two
consecutive runs, and a "memory optimisation" based on one sample turned out to halve the frame
rate while saving nothing. Report medians across runs, and say when the spread is bigger than the
difference you are claiming.

Also measure what Servo could never do: **start a voice call**. That is a real feature difference
and it is the kind of thing that decides this.

## What already exists and should be reused, not rewritten

In `D:\NEWProjects\WhatsAppRs\src`:

- `tray.rs`, `single_instance.rs`, `geometry.rs`, `paths.rs`, `shortcut.rs` are engine-independent
  and work today.
- `notify.rs` holds hard-won Windows notification knowledge: Windows will not render a toast without
  a Start Menu shortcut carrying an AppUserModelID, and WebView2 never hands a web notification to
  Windows on its own, so the host must draw it. Both facts cost hours. Keep them.
- `webview.rs` is safe mode on the OS webview and still builds. It is the fastest thing we have that
  works, and it is the fallback if both new candidates disappoint.
- `mode.rs` is the first-run picker between safe and light mode. With light mode retired it should
  probably become the picker between engines, or disappear.
- `tools/` holds every instrument used to produce the numbers, with a README explaining what each
  one proves and two traps that wasted real time.

## Gotchas that cost hours last time

- **A GUI app has no console, so a page error goes nowhere.** Forward the engine's console output to
  stderr from day one. A hung boot looked completely silent for hours until that existed.
- **Kill the app and you lose the login.** That was Servo-specific (it writes its cookie jar only at
  clean shutdown), but check it on any new engine before you kill a logged-in instance. Michael had
  to rescan a QR code three times because of this. `whatsapp.exe --quit` exists for the Servo build.
- **The build script renames a running binary aside**, because Windows will not let the linker
  replace a running exe. Keep that behaviour in whatever replaces it.
- **His phone shows the browser name.** The OS webview reports as Microsoft Edge, which he rejected
  outright. Chromium via CEF will report as Chrome, and Firefox as Firefox. Both are on WhatsApp's
  supported list; Edge is too, but he does not want it.

## How Michael wants you to work

- **Replies must be short.** He stopped reading long ones. Lead with the answer, plain English.
- **Measure, never assert.** Every number comes from a command you ran in that session.
- **Do it, do not recommend it**, when the action is reversible and within reach.
- He hates Chrome, Edge, Opera, Firefox as *browsers he has to run*, and Meta's official app. He is
  not contradicting himself by asking for a bundled Chromium or Firefox: the point is that the
  engine is ours to ship and control, not a browser he has to install and see.
- Do not open a visible console window; detached runs go hidden with output to a log.
