#!/usr/bin/env python3
"""Evaluate JavaScript in a running Firefox's page, over WebDriver BiDi.

The Chromium builds in this repo are driven with `cdp-eval.py`. Firefox cannot be:
**Firefox removed the Chrome DevTools Protocol entirely in version 141**, along with the
`remote.active-protocols` pref that used to enable it, so `--remote-debugging-port` on a
current Firefox starts the WebDriver BiDi remote agent instead. Same flag, different
protocol, and a CDP client against it fails in a way that looks like the browser is
broken rather than like the wrong protocol.

    python bidi-eval.py PORT file.js [--timeout 30]

Prints the returned value. The script's last expression is what comes back; wrap it in
an async IIFE and pass a promise if it needs to await.
"""

import json
import sys
import time

import websocket


class Bidi:
    def __init__(self, port, timeout=30):
        self.url = f"ws://127.0.0.1:{port}/session"
        # suppress_origin is not optional. Firefox's remote agent rejects the handshake
        # with "incorrect Origin header" if the client sends one, and websocket-client
        # sends one by default, so the connection fails with a 400 that reads like the
        # browser is broken rather than like a header problem.
        self.ws = websocket.create_connection(
            self.url, timeout=timeout, suppress_origin=True
        )
        self.next_id = 1

    def call(self, method, params=None, timeout=30):
        msg_id = self.next_id
        self.next_id += 1
        self.ws.send(json.dumps({"id": msg_id, "method": method, "params": params or {}}))
        deadline = time.time() + timeout
        while time.time() < deadline:
            frame = json.loads(self.ws.recv())
            # BiDi interleaves events with command responses; ignore anything that is
            # not the reply we are waiting for.
            if frame.get("id") != msg_id:
                continue
            if frame.get("type") == "error" or "error" in frame:
                raise RuntimeError(f"{method} failed: {frame}")
            return frame.get("result", {})
        raise TimeoutError(f"{method} did not answer within {timeout}s")

    def close(self):
        # Firefox allows ONE active BiDi session and does not free it the instant the
        # socket drops, so a second probe seconds later fails with "Maximum number of
        # active sessions" - which looks like the browser refusing to talk. Ending the
        # session explicitly is what makes back-to-back probes work.
        try:
            self.call("session.end", {}, timeout=5)
        except Exception:
            pass
        try:
            self.ws.close()
        except Exception:
            pass


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    port = int(sys.argv[1])
    script = open(sys.argv[2], encoding="utf-8").read()
    timeout = 30
    if "--timeout" in sys.argv:
        timeout = int(sys.argv[sys.argv.index("--timeout") + 1])

    bidi = Bidi(port, timeout)
    try:
        # A previous probe's session can still be closing. Retry rather than fail: this
        # is a race in Firefox's session teardown, not a real error.
        for attempt in range(6):
            try:
                bidi.call("session.new", {"capabilities": {}}, timeout)
                break
            except RuntimeError as err:
                if "Maximum number of active sessions" not in str(err) or attempt == 5:
                    raise
                time.sleep(1.0)
        tree = bidi.call("browsingContext.getTree", {}, timeout)
        contexts = tree.get("contexts", [])
        if not contexts:
            print(json.dumps({"error": "no browsing context"}))
            return 1
        # The app has exactly one tab; take the first top-level context.
        context = contexts[0]["context"]
        result = bidi.call(
            "script.evaluate",
            {
                "expression": script,
                "target": {"context": context},
                "awaitPromise": True,
            },
            timeout,
        )
        value = result.get("result", {})
        if value.get("type") == "string":
            print(value.get("value", ""))
        else:
            print(json.dumps(value, default=str))
        return 0
    finally:
        bidi.close()


if __name__ == "__main__":
    sys.exit(main())
