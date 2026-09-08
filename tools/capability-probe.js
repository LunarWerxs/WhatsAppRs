// Does the page still have everything WhatsApp needs?
//
// Run after any configuration change that could quietly remove a capability - most of all
// `--single-process`, which Chromium does not officially support and which puts the renderer
// inside the browser process. A configuration that saves memory by breaking the service
// worker, IndexedDB or WebRTC is not a saving, and none of those announce themselves: the
// page just stops syncing, or a call silently never connects.
//
// This is the same question the original `probe.rs` asked of WebView2 and WebKitGTK, kept in
// the same shape so the answers are comparable.
(function () {
  var out = {
    service_worker: !!navigator.serviceWorker,
    indexeddb: typeof indexedDB !== 'undefined',
    subtle_crypto: !!(window.crypto && window.crypto.subtle),
    webassembly: typeof WebAssembly !== 'undefined',
    shared_worker: typeof SharedWorker !== 'undefined',
    web_locks: !!(navigator.locks && navigator.locks.request),
    opfs: !!(navigator.storage && navigator.storage.getDirectory),
    media_devices: !!(navigator.mediaDevices && navigator.mediaDevices.getUserMedia),
    webrtc: typeof RTCPeerConnection !== 'undefined',
    notification: typeof Notification !== 'undefined',
    notification_permission: (typeof Notification !== 'undefined') ? Notification.permission : null,
    webgl: false,
    secure_context: window.isSecureContext
  };
  try {
    var c = document.createElement('canvas');
    out.webgl = !!(c.getContext('webgl2') || c.getContext('webgl'));
  } catch (e) {}

  if (!navigator.serviceWorker) return Promise.resolve(JSON.stringify(out));
  // Registered is not the same as controlling. WhatsApp's offline and notification paths
  // need a worker that is actually in charge of this page.
  return navigator.serviceWorker.getRegistration()
    .then(function (reg) {
      out.sw_registered = !!reg;
      out.sw_controlling = !!navigator.serviceWorker.controller;
      out.sw_state = reg && reg.active ? reg.active.state : null;
      return JSON.stringify(out);
    })
    .catch(function (e) {
      out.sw_error = String(e);
      return JSON.stringify(out);
    });
})()
