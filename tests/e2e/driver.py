#!/usr/bin/env python3
"""Drives kiki: talks to kikid over the socket (newline-delimited JSON) and to the shell
over `qs ipc`. Each check prints PASS/FAIL; exit code is the number of failures."""
import json, os, socket, subprocess, sys, time

sock_path, out_dir = sys.argv[1], sys.argv[2]
daemon_only = "--daemon-only" in sys.argv
home = os.environ["HOME_FIXTURE"]
fails = 0

class Daemon:
    def __init__(self, path):
        self.s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM); self.s.connect(path)
        self.f = self.s.makefile("rwb", buffering=0); self.n = 1; self.events = []
        self.call("Hello", version=1, client="e2e")
    def call(self, type, **fields):
        i = self.n; self.n += 1
        self.f.write((json.dumps(dict(id=i, type=type, **fields)) + "\n").encode())
        while True:
            line = self.f.readline()
            if not line: raise RuntimeError("daemon closed")
            m = json.loads(line)
            if m.get("id") == i: return m
            self.events.append(m)
    def drain(self, timeout=0.5):
        self.s.settimeout(timeout)
        try:
            while True:
                line = self.f.readline()
                if not line: break
                self.events.append(json.loads(line))
        except socket.timeout: pass
        self.s.settimeout(None)

def check(name, cond, detail=""):
    global fails
    print(("PASS " if cond else "FAIL ") + name + ("" if cond else "  " + str(detail)))
    if not cond: fails += 1

def ipc(*args):
    r = subprocess.run(["qs", "-p", os.environ.get("KIKI_SHELL_DIR", "qml") + "/shell.qml", "ipc", "call", "shell", *args], capture_output=True, text=True, timeout=10)
    return r.stdout.strip()

d = Daemon(sock_path)
# Listing: 10k files, first window, natural order, metadata arrives
t0 = time.time(); r = d.call("Open", lid=1, uri=f"file://{home}/big"); w = d.call("Window", lid=1, first=0, count=60); dt = (time.time() - t0) * 1000
check("open+window under 100 ms", dt < 100, f"{dt:.1f} ms")
rows = w["ok"]["rows"]; check("first row is file1.txt (natural order)", rows[0]["name"] == "file1.txt", rows[0]["name"])
d.drain(1.0)
check("count reached 10000", any(e.get("event") == "Count" and e.get("n") == 10000 for e in d.events))
check("metadata pushed for the live window", any(e.get("event") == "Rows" for e in d.events))
# Sort by size waits for enrichment
r = d.call("Sort", lid=1, role="size", order="desc"); check("sort by size replies after enrichment", "ok" in r, r)
# Jobs: trash and undo
r = d.call("Submit", op={"op": "trash", "items": [f"file://{home}/archive.tar"]}); job = r["ok"]["job"]
for _ in range(50):
    d.drain(0.1)
    if any(e.get("event") == "JobEvent" and e["job"]["id"] == job and e["job"]["state"] == "done" for e in d.events): break
check("trash job done", not os.path.exists(f"{home}/archive.tar"))
r = d.call("Undo"); time.sleep(0.5); check("undo restored the file", os.path.exists(f"{home}/archive.tar"))
# Preview of archive members
r = d.call("Preview", uri=f"file://{home}/archive.tar"); check("archive members preview", r.get("ok", {}).get("n", 0) >= 1, r)
# Search index
d.call("IndexRebuild"); time.sleep(2)
r = d.call("Search", lid=9, scope="everywhere", query="main.rs", mode="substring"); check("index finds main.rs", r.get("ok", {}).get("n", 0) >= 1, r)
w = d.call("Window", lid=9, first=0, count=5); check("search row carries parent", "parent" in (w["ok"]["rows"][0] if w["ok"]["rows"] else {}))
# Git status
subprocess.run(["git", "-C", f"{home}/Projects/kiki", "init", "-q"], check=False)
r = d.call("Repo", uri=f"file://{home}/Projects/kiki"); check("repo detected", r.get("ok") not in (None, {}), r)

if not daemon_only:
    ipc("open", f"file://{home}")
    time.sleep(0.5)
    st = json.loads(ipc("state") or "{}")
    check("shell shows the fixture home", st.get("uri") == f"file://{home}", st)
    ipc("setView", "icon"); time.sleep(0.3); check("icon view", json.loads(ipc("state")).get("view") == "icon")
    ipc("setView", "columns"); time.sleep(0.3); check("columns view", json.loads(ipc("state")).get("view") == "columns")
    ipc("search", "big"); time.sleep(0.5); check("folder filter", json.loads(ipc("state")).get("count") == 1)
    ipc("search", ""); ipc("inspector", "on"); time.sleep(0.3); check("inspector on", json.loads(ipc("state")).get("inspector") is True)
    ws = json.loads(ipc("windowState", "left") or "{}"); check("window cache holds a bounded set", 0 < ws.get("held", 0) <= 1200, ws)

print(f"{fails} failure(s)")
sys.exit(fails)
