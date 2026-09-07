(async () => {
  const out = { href: location.href, title: document.title, qrCanvas: !!document.querySelector('canvas') };
  try {
    const t = await (await fetch('/sw.js')).text();
    out.sw = { bytes: t.length, showNotification: (t.match(/showNotification/g) || []).length, newNotification: (t.match(/new Notification\(/g) || []).length };
  } catch (e) { out.sw = 'error: ' + e; }
  const urls = performance.getEntriesByType('resource').map(e => e.name).filter(u => /\.js(\?|$)/.test(u));
  out.scripts = urls.length;
  out.hits = [];
  let total = 0;
  for (const u of urls) {
    try {
      const t = await (await fetch(u)).text();
      total += t.length;
      const a = (t.match(/new Notification\(/g) || []).length;
      const b = (t.match(/showNotification/g) || []).length;
      if (a || b) out.hits.push({ script: u.replace(/^.*\//, '').slice(0, 60), newNotification: a, showNotification: b });
    } catch (e) {}
  }
  out.totalScriptBytes = total;
  return JSON.stringify(out);
})()
