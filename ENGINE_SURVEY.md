# Engine survey: can a lightweight engine run WhatsApp Web?

Date: 2026-09-07. Every row below was produced by actually loading `https://web.whatsapp.com` in
that engine on this machine and measuring it. Nothing is from documentation or memory. The owner's
target was roughly 10 MB, one process. The question was whether any non-Chromium engine gets there.

## Results

| Engine | Processes | Peak RAM | What WhatsApp did | Blocker |
| --- | --- | --- | --- | --- |
| **Chromium single-process** (WebView2, shipped in this app) | 3 total (1 ours + 2 engine) | **375 MB** | Full login page, notifications verified | none |
| **Servo 0.5.0** (Rust, not Chromium, not WebKit) | 1 | **635 MB** | Full layout rendered, QR square empty, storage fell back to no-op | no Cache Storage API (`caches`), and heavier than Chromium |
| **Ultralight 1.3** (WebKit 610, ~51 MB of DLLs) | 1 | **204 MB** | Loading splash only, then first error and stopped | no SubtleCrypto, no Service Workers, no Notifications |
| **Chromium with a phone user-agent** | 2 | 229 MB | Refused: "use WhatsApp Web from a browser on your computer" | WhatsApp does not serve the app to phones at all |

## The two user-agent gates, so nobody rediscovers them

WhatsApp gates on the user-agent STRING before it serves the app. Both alternative engines hit this
first, and it looks like an engine failure when it is not:

- Servo's default UA reads as Firefox and got "WhatsApp works with Mozilla Firefox 115+, update Firefox
  or use Chrome, Safari, Edge, Opera".
- Ultralight's WebKit 610 reads as an old Safari and got "WhatsApp works with Safari 15+".
- Presenting Ultralight as Chrome 152 produced a server-side "Sorry, something went wrong", most
  likely because a Chrome UA without Chrome's client-hint headers trips their fingerprinting.
- Presenting as a current Safari (the engine genuinely is WebKit) got the real app served.

Once past the gate, the real capability wall appears, and that is what the table records.

## Ultralight, measured from inside its own engine

Capability dump via `EvaluateScript`, on the served WhatsApp page:

```
sw:false  idb:true  subtle:false  wasm:true  caches:false  notif:false
ws:true   sharedWorker:false  localStorage:true  opfs:false  webrtc:false
```

`subtle:false` alone is fatal: WhatsApp's end-to-end encryption (Signal protocol) runs on
`crypto.subtle`. With no Service Workers there are no background alerts, and with no Notification
API there is nothing to alert with. These are engine gaps, not settings; Ultralight's README says it
omits "WebGL, WebRTC, and HTML5 Video/Audio" and tracks the rest in its issue tracker.

## Servo, closest of the alternatives

With `--pref dom_indexeddb_enabled=true --pref dom_serviceworker_enabled=true
--pref dom_notification_enabled=true` and a Chrome UA, Servo 0.5.0 rendered the whole login layout:
heading, three numbered steps, the "Stay logged in on this browser" checkbox, the create-account
form. Two things stopped it:

1. `caches.open is not a function`: the Cache Storage API does not exist in Servo and there is no
   pref for it (verified by string-searching the binary for `dom_cache*`). WhatsApp logged
   "Failed to initialize media store! Falling back to fake (no-op) storage" and never drew the QR.
2. **635 MB working set**, single process. That is 260 MB MORE than the shipped Chromium
   configuration. Servo is the most capable alternative and it loses on the one metric that matters.

## Where the memory actually goes, so the target is honest

Measured earlier in this project on Chromium: WhatsApp Web's JavaScript heap is 68.7 MB used /
97.1 MB allocated on a logged-out login screen, from 20 scripts totalling 25.3 MB of decoded code.
Even the refusal stub served to phones costs 229 MB in Chromium and 126 MB in Ultralight, which is
the floor of the engines themselves rendering almost nothing. The rust host process in this app is
23 MB. No engine that can run WhatsApp's code lands anywhere near 10 MB, because 70 to 100 MB of
that is WhatsApp's own heap before any renderer exists.

## Verdict

**No lightweight engine can run WhatsApp Web as of 2026-09-07, and this is now measured rather than
asserted.** The lightest configuration that actually works is the one already shipped: Chromium
single-process at 375 MB across 3 processes.

If Servo ships Cache Storage (it is the only missing piece for the login page to fully load, and the
project is moving fast), it becomes the first non-Chromium, non-WebKit engine that runs WhatsApp Web,
and re-testing takes five minutes with the commands below. It would still need to lose 300 MB to
beat what is shipped.

## Reproducing

- Servo: `servoshell.exe --window-size 1180x860 -u "<Chrome UA>" --pref dom_indexeddb_enabled=true --pref dom_serviceworker_enabled=true --pref dom_notification_enabled=true -o out.png https://web.whatsapp.com`
  (release v0.5.0, `servo-x86_64-windows-msvc.zip`, sha256 verified against the published file).
- Ultralight: SDK `ultralight-sdk-latest-win-x64.7z`, build `samples/Sample 1 - Render to PNG` from
  the SDK root with the VS 2022 cmake, set `view_config.user_agent` to a current Safari string, pump
  `Update()/Render()` for 15 s after `OnFinishLoading`, and dump capabilities via `EvaluateScript`.
- Mobile UA: `bench.exe` in this repo with `WA_UA` set to an Android Chrome or iOS Safari string.
