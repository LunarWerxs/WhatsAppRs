"""Evaluate a JS expression (awaited) in the running webview's page over CDP.
usage: python cdp-eval.py PORT FILE_WITH_EXPRESSION
"""
import json
import sys
import urllib.request

import websocket

port, path = sys.argv[1], sys.argv[2]
expr = open(path, encoding="utf-8").read()
targets = json.load(urllib.request.urlopen(f"http://127.0.0.1:{port}/json"))
page = next(t for t in targets if t["type"] == "page")
ws = websocket.create_connection(page["webSocketDebuggerUrl"], timeout=120)
ws.send(json.dumps({"id": 1, "method": "Runtime.evaluate",
                    "params": {"expression": expr, "awaitPromise": True, "returnByValue": True}}))
while True:
    msg = json.loads(ws.recv())
    if msg.get("id") == 1:
        break
ws.close()
res = msg.get("result", {}).get("result", {})
print(res.get("value", msg))
