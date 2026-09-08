// What the WhatsApp Web page currently is, in one line, for either engine.
//
// The harness polls this to decide when a run has actually STARTED, rather than
// sampling memory at a fixed number of seconds and hoping. "60 seconds after launch" and
// "60 seconds after the chat list appeared" are different measurements, and only the
// second one compares two engines fairly.
(function () {
  var text = (document.body && document.body.innerText) || '';
  var rows = document.querySelectorAll('[role="listitem"], [data-testid="cell-frame-container"]').length;
  var qr = !!document.querySelector('canvas, [data-ref], [aria-label*="QR" i]') &&
           /scan|qr code|link.*device/i.test(text);
  var state = 'loading';
  if (rows > 0) state = 'chats';
  else if (/loading your chats/i.test(text)) state = 'syncing';
  else if (qr || /scan to log in|log in with phone number/i.test(text)) state = 'qr';
  var heap = null;
  try {
    // Chromium only. Firefox has no performance.memory, so this stays null there and the
    // comparison uses the process numbers, which both engines report the same way.
    if (performance && performance.memory) {
      heap = {
        used_mb: Math.round(performance.memory.usedJSHeapSize / 1048576),
        total_mb: Math.round(performance.memory.totalJSHeapSize / 1048576)
      };
    }
  } catch (e) {}
  return JSON.stringify({
    state: state,
    rows: rows,
    title: document.title,
    ready: document.readyState,
    heap: heap,
    ua_brand: navigator.userAgent.replace(/^.*?(Firefox|Chrome|Edg)\/([\d.]+).*$/, '$1 $2')
  });
})()
