// Did the notification shim reach the page?
//
// The bridge in cef_view.rs injects itself from the RENDER process, which is a different
// process from the one that holds the window, so "it compiled" says nothing about whether it
// ran. This asks the page directly, and reports what the Notification object actually is -
// the engine's own, ours, or missing entirely.
(function () {
  var out = {
    bridged: !!window.__waRsNotifyBridged,
    notification_type: typeof Notification,
    permission: (typeof Notification !== 'undefined') ? Notification.permission : null
  };
  try {
    // A native constructor stringifies with "[native code]"; ours does not. That is the
    // difference between the engine answering and our shim answering.
    out.notification_source = (typeof Notification !== 'undefined')
      ? (/\[native code\]/.test(Function.prototype.toString.call(Notification)) ? 'engine' : 'ours')
      : 'absent';
  } catch (e) { out.notification_source = 'unknown: ' + e; }
  out.sw_show_patched = !!(window.ServiceWorkerRegistration &&
    ServiceWorkerRegistration.prototype.showNotification &&
    !/\[native code\]/.test(Function.prototype.toString.call(ServiceWorkerRegistration.prototype.showNotification)));
  return Promise.resolve(JSON.stringify(out));
})()
