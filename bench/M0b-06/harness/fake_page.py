#!/usr/bin/env python3
"""Stand-in for the test window, for a dry run of evidence.sh without a person and without a
browser: it reads /state from the loopback server and answers like the page would (reports a
full-screen size, then a key after the key step, a click after each click step). It sends no
input to the system; it only makes requests to the loopback server.
Usage: fake_page.py <port>
"""
import json
import sys
import time
import urllib.request

PORT = sys.argv[1]
BASE = "http://127.0.0.1:%s" % PORT


def get(path):
    with urllib.request.urlopen(BASE + path, timeout=5) as r:
        return r.read()


def log(query):
    get("/log?" + query)


log("ev=FULLSCREEN&v=yes&iw=3440&ih=1440&sw=3440&sh=1440")
seen = None
while True:
    try:
        st = json.loads(get("/state"))
    except (OSError, ValueError):
        time.sleep(0.3)
        continue
    if st.get("seq") != seen:
        seen = st.get("seq")
        target = st.get("target")
        if target:
            log("ev=TARGET&x=%s&y=%s&size=%s" % (target["x"], target["y"], target["size"]))
        step = st.get("step", "")
        log("ev=STEP&name=" + step)
        time.sleep(0.5)
        if step == "fullscreen":
            log("ev=CLICK&x=10&y=10")
        elif step == "key":
            log("ev=KEY&key=KeyA")
        elif step == "click1" and target:
            log("ev=CLICK&x=%s&y=%s" % (target["x"], target["y"]))
        # click2 is the control: the bar catches the click, so the page logs nothing
    time.sleep(0.25)
