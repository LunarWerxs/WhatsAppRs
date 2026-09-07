# The memory question, and the only architecture that answers it

Written 2026-09-06 after the owner rejected 633 MB RAM and a 58 MB data directory as a failure,
with a target of roughly 1 to 10 MB, one process, and a 1 to 2 MB data directory.

## Where the 633 MB actually goes

Measured, not estimated:

| Component | Memory | Whose code |
| --- | --- | --- |
| Our Rust process | **24 MB** | ours |
| Chromium running WhatsApp Web | **609 MB** | WhatsApp's |

**Our code is 24 MB of the 633 MB.** The rest is a browser engine executing WhatsApp's JavaScript.
Rewriting our part perfectly to zero bytes would take 633 MB down to 609 MB.

## Why the data directory is 58 MB

Measured on a fresh profile showing nothing but the **logged-out QR screen**:

| Item | Size |
| --- | --- |
| Service worker script cache | 14.5 MB |
| HTTP cache | 12.5 MB |
| Shader / GPU caches | 6.2 MB |
| Everything else | ~3 MB |

That is about **27 MB of WhatsApp's own JavaScript and assets, cached, before you have even logged
in**. WhatsApp Web is a large application. Any engine that runs it stores that code.

## Why "lightweight browsers exist" does not solve it

It is true that small browsers exist. It is not true that any of them can run WhatsApp Web.

- **Small mobile browsers** (Via, Lightning, and similar) are small *downloads* that call the
  operating system's built-in engine. That is exactly the architecture used here, and this app's
  download is **786 KB**. Their RAM usage is the system engine's, in the hundreds of MB, same as ours.
- **Genuinely small engines** (Sciter, Ultralight, NetSurf, Dillo) are small because they omit the
  machinery WhatsApp Web requires: service workers, IndexedDB, WebAssembly, full modern JavaScript.
  They cannot load it at all.
- **Servo** is Rust and embeddable but nowhere near able to render WhatsApp Web.

So within the "embed a webview" architecture, several hundred MB is a floor set by WhatsApp, not a
number that better engineering moves. Tuning Chromium switches (disabling the GPU process, capping
the JS heap, forcing one renderer, WebView2's low memory-usage target) is worth doing and is not yet
done, but it is realistically a **cut to roughly 350 to 450 MB, not to 10 MB**.

## The only architecture that reaches the target: no browser at all

Stop running WhatsApp's JavaScript. Speak WhatsApp's protocol directly, natively, in Rust, and draw
a native UI. No engine, one process, and the data directory becomes a message database rather than a
browser cache. This plausibly lands in the tens of MB with a single process.

Mature Rust libraries for this exist today:

- [oxidezap/whatsapp-rust](https://github.com/oxidezap/whatsapp-rust): v0.7, described as
  production-ready, 1,681 commits, 739 stars, 125 forks. Noise handshake, Signal protocol, QR
  pairing, media, groups, communities, newsletters, even voice calls.
- [crates.io/crates/whatsapp-rust](https://crates.io/crates/whatsapp-rust) and the `wa-rs` fork for
  stable-Rust builds.

## CORRECTION, 2026-09-06: the browser is not the risk, and never was

An earlier version of this document let "ban risk" bleed across both architectures. That was wrong
and the owner caught it. The two are not on a spectrum, they are on opposite sides of a specific
clause. Verified against primary sources:

**WhatsApp's Terms of Service contain no restriction whatsoever on which browser you use.** The
clause that matters is under Acceptable Use, quoted verbatim:

> "reverse engineer, alter, modify, create derivative works from, decompile, or extract code from
> our Services"

**Officially supported browsers for WhatsApp Web are Chrome, Firefox, Edge, Opera and Safari.**
Opera is on WhatsApp's own supported list.

So:

| | What it does | Which clause it touches |
| --- | --- | --- |
| **This app (webview wrapper)** | Loads web.whatsapp.com, Meta's real web app, in a real browser engine | **None.** Identical in kind to using Firefox or Opera. |
| **Native protocol client** | Does not load their site at all; reimplements their wire protocol | **Exactly the reverse-engineering clause above.** |

Using a third-party browser, or an app that embeds a browser engine pointed at their real website,
is not a Terms violation and is not what gets accounts banned. What gets banned is a client that
reimplements the protocol (whatsapp-rust, whatsmeow, Baileys) or a modified WhatsApp app
(GB WhatsApp, WhatsApp Plus). Meta's own stated reason is that they cannot vouch for code they do
not control, which is a statement about reimplementations, not about renderers.

**Consequence for the whatRust issue #17 anecdote** recorded in FINDINGS.md: that user was on a
webview wrapper, which by the above touches no clause, and their account was already suspended and
under repeated appeal. The confounder is now clearly the better explanation. It should not be
treated as evidence that wrappers get banned.

## The cost, quoted rather than paraphrased

From that project's own README:

> "This is an unofficial, open-source reimplementation. Using custom WhatsApp clients may violate
> Meta's Terms of Service and could result in account suspension. Use at your own risk."

This is a **materially higher risk class than a webview wrapper**, and the distinction is the whole
point. A wrapper loads Meta's real web app in a real browser engine. A protocol client
reimplements their protocol, which is precisely the thing their anti-abuse systems detect.

Corroborating evidence, not speculation:

- [whatsmeow issue #810](https://github.com/tulir/whatsmeow/issues/810): "Your account may be at
  risk" warnings hitting clients that were only replying to incoming messages, not sending bulk.
- [Baileys issue #2309](https://github.com/WhiskeySockets/Baileys/issues/2309): a permanent ban.
- Reporting on unofficial-API bans is consistent that they are **permanent with no effective
  appeal**, and that detection lands within weeks to months rather than never.

## Recommendation

Build the native protocol client, because it is the only thing that reaches the target, but
**pair it with a phone number that is not the owner's primary account**. That buys the 10-to-50 MB
single-process architecture he actually wants while quarantining the one consequence that cannot be
undone. Keep the webview build as the safe fallback that works today.

The decision is the owner's, because the risk is his account, not a technical trade.
