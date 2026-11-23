#!/usr/bin/env python3
"""Loopback test server for the process tap spike.

Serves page.html and the test clip, and reports the playback mode that the harness wrote
to a state file. Binds to 127.0.0.1 only. Usage: serve.py <state_file> <clip_dir> <port_file>
"""
import http.server
import json
import os
import sys

STATE_FILE, CLIP_DIR, PORT_FILE = sys.argv[1:4]
PAGE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "page.html")


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def _send(self, code, body, ctype):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path in ("/", "/index.html"):
            with open(PAGE, "rb") as f:
                self._send(200, f.read(), "text/html; charset=utf-8")
        elif self.path == "/clip.wav":
            with open(os.path.join(CLIP_DIR, "speech.wav"), "rb") as f:
                self._send(200, f.read(), "audio/wav")
        elif self.path == "/state":
            try:
                with open(STATE_FILE) as f:
                    state = json.loads(f.read() or "{}")
            except (OSError, ValueError):
                state = {}
            state.setdefault("mode", "off")
            self._send(200, json.dumps(state).encode(), "application/json")
        else:
            self._send(404, b"not found", "text/plain")


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
with open(PORT_FILE, "w") as f:
    f.write(str(server.server_address[1]))
server.serve_forever()
