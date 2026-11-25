#!/usr/bin/env python3
"""Loopback server of the test window of the panel spike.

Serves page.html, the current step (`/state`, read from a file the harness writes) and a log
endpoint (`/log?ev=KEY&key=KeyA`) that appends `<epoch ms> <EVENT> k=v ...` to the page log.
Binds to 127.0.0.1 only. Usage: serve.py <state_file> <page_log> <port_file>
"""
import http.server
import os
import re
import sys
import time
import urllib.parse

STATE_FILE, PAGE_LOG, PORT_FILE = sys.argv[1:4]
PAGE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "page.html")
SAFE = re.compile(r"[^A-Za-z0-9._,%-]")
DEFAULT_STATE = b'{"seq": 0, "step": "wait", "text": "Waiting for the harness."}'


def clean(text):
    return SAFE.sub("", text)[:60]


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
        url = urllib.parse.urlsplit(self.path)
        if url.path in ("/", "/index.html"):
            with open(PAGE, "rb") as f:
                self._send(200, f.read(), "text/html; charset=utf-8")
        elif url.path == "/state":
            try:
                with open(STATE_FILE, "rb") as f:
                    body = f.read() or DEFAULT_STATE
            except OSError:
                body = DEFAULT_STATE
            self._send(200, body, "application/json")
        elif url.path == "/log":
            query = urllib.parse.parse_qsl(url.query)
            fields = dict(query)
            event = clean(fields.pop("ev", ""))
            if event:
                rest = " ".join("%s=%s" % (clean(k), clean(v)) for k, v in fields.items())
                with open(PAGE_LOG, "a") as f:
                    f.write("%d %s %s\n" % (int(time.time() * 1000), event, rest))
            self._send(204, b"", "text/plain")
        else:
            self._send(404, b"not found", "text/plain")


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
with open(PORT_FILE, "w") as f:
    f.write(str(server.server_address[1]))
server.serve_forever()
