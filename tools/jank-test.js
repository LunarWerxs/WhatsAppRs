(function () {
  var log = function (m) { window.__wa_errors.push('JANK ' + m); };
  // Let the account finish syncing before judging how smooth it feels.
  setTimeout(function () {
    var frames = [];
    var last = performance.now();
    var started = last;
    var tick = function (now) {
      frames.push(now - last);
      last = now;
      if (now - started < 6000) {
        requestAnimationFrame(tick);
        return;
      }
      frames.sort(function (a, b) { return a - b; });
      var n = frames.length;
      if (!n) { log('no frames at all'); return; }
      var sum = frames.reduce(function (a, b) { return a + b; }, 0);
      var at = function (p) { return frames[Math.min(n - 1, Math.floor(n * p))].toFixed(1); };
      log('frames=' + n +
          ' fps=' + (1000 / (sum / n)).toFixed(1) +
          ' median=' + at(0.5) + 'ms' +
          ' p95=' + at(0.95) + 'ms' +
          ' worst=' + frames[n - 1].toFixed(0) + 'ms' +
          ' over16ms=' + frames.filter(function (f) { return f > 16.7; }).length);
    };
    requestAnimationFrame(tick);
  }, 30000);
  return 'jank probe armed';
})()
