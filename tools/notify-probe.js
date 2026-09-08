// Does this engine actually raise a desktop notification?
//
// The lesson from the WebView2 round, which cost a day and produced a claim that was
// simply false: **the page's own report is worth nothing.** `onshow` fired and
// `getNotifications()` said "displayed" while Windows' notification database recorded
// nothing and no toast ever appeared on screen. So this script reports only what the page
// can see, and `toast-db.py` is the ground truth that has to agree with it.
//
// Two paths, because they fail independently:
//   - page context, `new Notification(...)`  - the one WhatsApp Web's loaded code uses
//   - service worker, `registration.showNotification(...)` - the one it also references
(function () {
  var out = { permission_before: (typeof Notification !== 'undefined') ? Notification.permission : 'no Notification API' };
  if (typeof Notification === 'undefined') {
    return Promise.resolve(JSON.stringify(out));
  }

  function pagePath() {
    return new Promise(function (resolve) {
      var n;
      try {
        n = new Notification('WHATSAPP-RS-PAGE', { body: 'page context probe', tag: 'wa-rs-page' });
      } catch (e) {
        resolve({ page: 'threw: ' + e }); return;
      }
      var settled = false;
      n.onshow  = function () { if (!settled) { settled = true; resolve({ page: 'onshow' }); } };
      n.onerror = function (e) { if (!settled) { settled = true; resolve({ page: 'onerror' }); } };
      setTimeout(function () { if (!settled) { settled = true; resolve({ page: 'no event in 4s' }); } }, 4000);
    });
  }

  function swPath() {
    if (!navigator.serviceWorker) return Promise.resolve({ sw: 'no serviceWorker' });
    return navigator.serviceWorker.getRegistration()
      .then(function (reg) {
        if (!reg) return { sw: 'no registration' };
        return reg.showNotification('WHATSAPP-RS-SW', { body: 'service worker probe', tag: 'wa-rs-sw' })
          .then(function () { return reg.getNotifications({ tag: 'wa-rs-sw' }); })
          .then(function (list) { return { sw: 'resolved', sw_listed: list.length }; })
          .catch(function (e) { return { sw: 'rejected: ' + e }; });
      })
      .catch(function (e) { return { sw: 'getRegistration failed: ' + e }; });
  }

  return Notification.requestPermission()
    .then(function (p) { out.permission_after_request = p; })
    .catch(function (e) { out.permission_after_request = 'threw: ' + e; })
    .then(pagePath)
    .then(function (r) { Object.assign(out, r); return swPath(); })
    .then(function (r) { Object.assign(out, r); })
    .then(function () {
      out.permission_final = Notification.permission;
      return JSON.stringify(out);
    });
})()
