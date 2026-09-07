"""Fire a test notification inside the running WhatsApp Rs webview over its
debugging port (WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=N).

usage: python cdp-notify.py PORT page|sw|both
Prints the page's own view of what happened; whether Windows actually rendered a
toast is answered by toast-watch.ps1, not by this.
"""
import json
import sys
import urllib.request

import websocket

port = sys.argv[1] if len(sys.argv) > 1 else "9333"
what = sys.argv[2] if len(sys.argv) > 2 else "both"

targets = json.load(urllib.request.urlopen(f"http://127.0.0.1:{port}/json"))
page = next(t for t in targets if t["type"] == "page")

PAGE_JS = r"""
(async () => {
  const r = { permissionBefore: Notification.permission, when: new Date().toISOString() };
  try { r.requested = await Notification.requestPermission(); } catch (e) { r.requested = 'error: ' + e; }
  r.permission = Notification.permission;
  if (WHAT === 'page' || WHAT === 'both') {
    try {
      const n = new Notification('WhatsApp Rs test (page)', { body: 'page-context toast ' + r.when, tag: 'rs-page' });
      r.page = 'constructed';
      await new Promise(res => { n.onshow = () => { r.page = 'onshow fired'; res(); }; n.onerror = e => { r.page = 'onerror'; res(); }; setTimeout(res, 3000); });
    } catch (e) { r.page = 'error: ' + e; }
  }
  if (WHAT === 'sw' || WHAT === 'both') {
    try {
      const reg = await navigator.serviceWorker.ready;
      await reg.showNotification('WhatsApp Rs test (worker)', { body: 'service-worker toast ' + r.when, tag: 'rs-sw' });
      const list = await reg.getNotifications({ tag: 'rs-sw' });
      r.sw = list.length ? 'displayed per getNotifications' : 'dropped per getNotifications';
    } catch (e) { r.sw = 'error: ' + e; }
  }
  return JSON.stringify(r);
})()
""".replace("WHAT", json.dumps(what))

ws = websocket.create_connection(page["webSocketDebuggerUrl"], timeout=20)
ws.send(json.dumps({"id": 1, "method": "Runtime.evaluate",
                    "params": {"expression": PAGE_JS, "awaitPromise": True, "returnByValue": True}}))
while True:
    msg = json.loads(ws.recv())
    if msg.get("id") == 1:
        break
ws.close()
res = msg.get("result", {}).get("result", {})
print("page says:", res.get("value", msg))
