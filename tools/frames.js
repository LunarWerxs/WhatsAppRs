// Frame timing for a WhatsApp Web window, engine-independent.
//
// Resolves with a JSON string, so it works unchanged through CDP's
// Runtime.evaluate (awaitPromise) for the Chromium build and through WebDriver BiDi's
// script.evaluate (awaitPromise) for the Firefox build. Same numbers, same code, which
// is the only way a head-to-head means anything.
//
// It SCROLLS while measuring. An idle page paints nothing and every engine reports the
// monitor's refresh rate, which measures nothing; the interesting number is what happens
// when the chat list is actually moving. With no chat list (a logged-out window) it
// falls back to measuring idle frames and says so.
(function () {
  var DURATION = 6000;

  function findScroller() {
    var best = null;
    var nodes = document.querySelectorAll('div');
    for (var i = 0; i < nodes.length; i++) {
      var el = nodes[i];
      if (el.scrollHeight > el.clientHeight + 200 && el.clientHeight > 200) {
        if (!best || el.clientHeight > best.clientHeight) best = el;
      }
    }
    return best;
  }

  return new Promise(function (resolve) {
    var scroller = findScroller();
    var frames = [];
    var last = performance.now();
    var started = last;
    var dir = 1;

    function tick(now) {
      frames.push(now - last);
      last = now;
      if (scroller) {
        // Oscillate rather than run to the end, so the work continues for the whole run.
        scroller.scrollTop += dir * 40;
        if (scroller.scrollTop <= 0) dir = 1;
        if (scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 1) dir = -1;
      }
      if (now - started < DURATION) {
        requestAnimationFrame(tick);
        return;
      }
      frames.sort(function (a, b) { return a - b; });
      var n = frames.length;
      if (!n) { resolve(JSON.stringify({ error: 'no frames' })); return; }
      var sum = frames.reduce(function (a, b) { return a + b; }, 0);
      var at = function (p) { return +frames[Math.min(n - 1, Math.floor(n * p))].toFixed(1); };
      resolve(JSON.stringify({
        driven: !!scroller,
        frames: n,
        fps: +(1000 / (sum / n)).toFixed(1),
        median_ms: at(0.5),
        p95_ms: at(0.95),
        worst_ms: +frames[n - 1].toFixed(0),
        over16: frames.filter(function (f) { return f > 16.7; }).length
      }));
    }
    requestAnimationFrame(tick);
  });
})()
