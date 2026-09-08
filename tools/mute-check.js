// Does the tray's "Mute sounds" reach the page, and does it mute only the tones?
//
// The shim patches HTMLMediaElement.prototype.play so that a detached element playing a
// plain https source (which is what every WhatsApp alert tone is) is muted while the toggle
// is on, and nothing else is. This exercises exactly that rule with three elements:
//   tone   - detached, https src        -> muted while on, not muted while off
//   inDom  - attached to the document   -> never touched
//   blob   - detached, blob: src        -> never touched (voice messages play from blobs)
// It also reports the state the host pushed at load, so a pre-seeded settings.txt with
// mute_sounds=1 shows up as pushed_at_load=true.
(function () {
  var out = {
    set_mute_fn: typeof window.__waRsSetMute,
    patched: !!(window.HTMLMediaElement && HTMLMediaElement.prototype.__waRsMutePatched),
    pushed_at_load: window.__waRsMuted === true
  };
  function tryPlay(el) { try { var p = el.play(); if (p && p.catch) p.catch(function () {}); } catch (e) {} }
  function round(on) {
    window.__waRsSetMute(on);
    var tone = new Audio('https://web.whatsapp.com/favicon.ico');
    var inDom = document.createElement('audio'); inDom.src = 'https://web.whatsapp.com/favicon.ico';
    document.body.appendChild(inDom);
    var blob = new Audio(URL.createObjectURL(new Blob([new Uint8Array(4)], { type: 'audio/ogg' })));
    tryPlay(tone); tryPlay(inDom); tryPlay(blob);
    var r = { tone: tone.muted, inDom: inDom.muted, blob: blob.muted };
    try { document.body.removeChild(inDom); } catch (e) {}
    return r;
  }
  if (typeof window.__waRsSetMute === 'function') {
    out.on = round(true);
    out.off = round(false);
    out.verdict = (out.on.tone === true && out.on.inDom === false && out.on.blob === false &&
                   out.off.tone === false) ? 'PASS' : 'FAIL';
  } else {
    out.verdict = 'FAIL: no __waRsSetMute';
  }
  return Promise.resolve(JSON.stringify(out));
})()
