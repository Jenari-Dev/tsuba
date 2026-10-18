#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Tiny stand-in for tsubad's game lighting REST server, for the bridge DLL tests.

    mock_server.py serve PORTFILE LOGFILE   listen on 127.0.0.1:<ephemeral>, write the port to
                                            PORTFILE, log every request as a JSON line to LOGFILE
    mock_server.py check LOGFILE            assert the requests the harness should have caused

Like the real server, every session gets its own listener whose URI (with a per-session token in
the path) is returned by the init POST; requests that don't follow that URI get a 404.
"""
import json
import secrets
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

LOCK = threading.Lock()
LOG = None
SESSIONS = {}


def log(entry):
    with LOCK:
        LOG.write(json.dumps(entry) + "\n")
        LOG.flush()


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"  # keep-alive, like the real server

    def log_message(self, *a):
        pass

    def reply(self, obj, status=200):
        data = json.dumps(obj).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def handle_any(self):
        n = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(n) if n else b""
        try:
            body = json.loads(raw) if raw else None
        except ValueError:
            body = {"_invalid_json": raw.decode(errors="replace")}
        port = self.server.server_address[1]
        path = self.path
        if self.server.session is not None:
            base = self.server.base
            if path != base and not path.startswith(base + "/"):
                log({"port": port, "session": self.server.session, "method": self.command, "path": path, "body": body, "untokened": True})
                return self.reply({"result": 1168}, 404)
            path = "/chromasdk" + path[len(base):]
        self.path = path
        log({"port": port, "session": self.server.session, "method": self.command, "path": path, "body": body})
        if self.server.session is None:
            if self.command == "POST" and self.path == "/razer/chromasdk":
                sid = len(SESSIONS) + 1
                srv = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
                srv.session = sid
                srv.base = "/chromasdk/" + secrets.token_hex(16)
                srv.daemon_threads = True
                SESSIONS[sid] = srv
                threading.Thread(target=srv.serve_forever, daemon=True).start()
                return self.reply({"sessionid": sid, "uri": f"http://localhost:{srv.server_address[1]}{srv.base}"})
            return self.reply({"result": 1168}, 404)
        if self.path == "/chromasdk/heartbeat":
            return self.reply({"tick": 1})
        if self.command == "DELETE" and self.path == "/chromasdk":
            return self.reply({"result": 0})
        return self.reply({"result": 0})

    do_GET = do_POST = do_PUT = do_DELETE = handle_any


def serve(portfile, logfile):
    global LOG
    LOG = open(logfile, "a")
    srv = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    srv.session = None
    srv.daemon_threads = True
    with open(portfile, "w") as f:
        f.write(str(srv.server_address[1]))
    srv.serve_forever()


def check(logfile):
    reqs = [json.loads(l) for l in open(logfile) if l.strip()]
    errors = []

    def expect(cond, msg):
        if not cond:
            errors.append(msg)

    for r in reqs:
        expect(not r.get("untokened"), f"request outside the session uri: {r['method']} {r['path']}")
    inits = [r for r in reqs if r["method"] == "POST" and r["path"] == "/razer/chromasdk"]
    # One session per harness run (x64 + x86).
    expect(len(inits) >= 1, "no init POST")
    for r in inits:
        b = r["body"]
        expect(b["title"] == "Harness Game — 剣", f"title {b['title']!r}")
        expect(b["author"] == {"name": "JenAriDev", "contact": "https://example.invalid"}, f"author {b['author']}")
        expect(b["device_supported"] == ["keyboard", "mouse"], f"devices {b['device_supported']}")
        expect(b["category"] == "game", f"category {b['category']}")
    for sid in sorted({r["session"] for r in reqs if r["session"] is not None}):
        s = [r for r in reqs if r["session"] == sid]
        dev = lambda d: [r["body"] for r in s if r["method"] == "PUT" and r["path"] == f"/chromasdk/{d}"]
        kb = dev("keyboard")
        expect(kb and kb[0]["effect"] == "CHROMA_CUSTOM", f"s{sid}: first keyboard frame {kb[:1]}")
        if kb and kb[0]["effect"] == "CHROMA_CUSTOM":
            g = kb[0]["param"]
            expect(len(g) == 6 and all(len(row) == 22 for row in g), f"s{sid}: custom grid shape")
            expect(all(g[r][c] == r * 16 + c for r in range(6) for c in range(22)), f"s{sid}: custom grid values")
        expect(len(kb) < 150, f"s{sid}: {len(kb)} keyboard frames for a 200-frame burst (no coalescing?)")
        ck = [b for b in kb if b["effect"] == "CHROMA_CUSTOM_KEY"]
        expect(ck and ck[-1]["param"]["key"][2][3] == 0x010000FF, f"s{sid}: custom-key frame/key flag")
        last = kb[-1] if kb else {}
        expect(last.get("effect") == "CHROMA_CUSTOM2", f"s{sid}: last keyboard frame {last.get('effect')}")
        if last.get("effect") == "CHROMA_CUSTOM2":
            p = last["param"]
            expect(len(p["color"]) == 8 and len(p["color"][0]) == 24 and p["color"][7][23] == 0x654321, f"s{sid}: custom2 colour grid")
            expect(len(p["key"]) == 6 and len(p["key"][0]) == 22, f"s{sid}: custom2 key grid")
        expect(dev("mouse") == [{"effect": "CHROMA_STATIC", "param": {"color": 65280}}], f"s{sid}: mouse {dev('mouse')}")
        expect(dev("mousepad") == [{"effect": "CHROMA_CUSTOM", "param": [255, 65280, 16711680, 16777215, 0x123456] + [0] * 10}], f"s{sid}: mousepad {dev('mousepad')}")
        expect(dev("headset") == [{"effect": "CHROMA_CUSTOM", "param": [255, 65280, 16711680, 16777215, 0x123456]}], f"s{sid}: headset {dev('headset')}")
        kp = dev("keypad")
        expect(len(kp) == 1 and kp[0]["param"][3][4] == 0xABCDEF and len(kp[0]["param"]) == 4, f"s{sid}: keypad {kp}")
        expect(len(dev("chromalink")) == 1, f"s{sid}: chromalink {dev('chromalink')}")
        hb = [r for r in s if r["path"] == "/chromasdk/heartbeat" and r["method"] == "PUT"]
        expect(len(hb) >= 2, f"s{sid}: {len(hb)} heartbeats")
        expect(s[-1]["method"] == "DELETE" and s[-1]["path"] == "/chromasdk", f"s{sid}: last request {s[-1]['method']} {s[-1]['path']}")
        print(f"session {sid}: {len(s)} requests, {len(kb)} keyboard frames, {len(hb)} heartbeats, closed with DELETE")
    if errors:
        print("\n".join("FAIL: " + e for e in errors))
        sys.exit(1)
    print("mock check: ok")


if __name__ == "__main__":
    if sys.argv[1] == "serve":
        serve(sys.argv[2], sys.argv[3])
    else:
        check(sys.argv[2])
