"""Grep the JavaScript the page has actually loaded, via the debugger, not the network.

usage: python script-grep.py PORT REGEX [CONTEXT_CHARS]

Asks CDP for every parsed script's source (Debugger.getScriptSource) and prints each match
with some context and the script URL. This is how "how does WhatsApp play its notification
sound" gets answered from the running page instead of from guesswork: the bundles are
cross-origin, so fetch() from the page cannot read them, but the debugger can.
"""
import json
import re
import sys
import urllib.request

import websocket

port, pattern = sys.argv[1], sys.argv[2]
context = int(sys.argv[3]) if len(sys.argv) > 3 else 160
rx = re.compile(pattern)

targets = json.load(urllib.request.urlopen(f"http://127.0.0.1:{port}/json"))
page = next(t for t in targets if t["type"] == "page")
ws = websocket.create_connection(page["webSocketDebuggerUrl"], timeout=120, suppress_origin=True)

seq = 0


def call(method, params=None):
    global seq
    seq += 1
    ws.send(json.dumps({"id": seq, "method": method, "params": params or {}}))
    events = []
    while True:
        msg = json.loads(ws.recv())
        if msg.get("id") == seq:
            return msg.get("result", {}), events
        events.append(msg)


_, events = call("Debugger.enable")
# scriptParsed events arrive as a burst right after enable; drain a little more.
ws.settimeout(2)
try:
    while True:
        events.append(json.loads(ws.recv()))
except Exception:
    pass
ws.settimeout(120)

scripts = [e["params"] for e in events if e.get("method") == "Debugger.scriptParsed"]
print(f"{len(scripts)} scripts parsed", file=sys.stderr)
hits = 0
for s in scripts:
    res, _ = call("Debugger.getScriptSource", {"scriptId": s["scriptId"]})
    src = res.get("scriptSource", "")
    for m in rx.finditer(src):
        hits += 1
        lo, hi = max(0, m.start() - context), min(len(src), m.end() + context)
        print(f"--- {s.get('url', '?')[-80:]} @{m.start()}")
        print(src[lo:hi].replace("\n", " "))
        if hits >= 60:
            print("... (stopped at 60 hits)")
            sys.exit(0)
ws.close()
print(f"{hits} hits", file=sys.stderr)
