#!/usr/bin/env python3
# octobuddy-app-drive: runs an OctoSense app headless and drives it as a person would,
# step by step, then says what each step did and what is on screen (OctoBuddy writes
# this file). Usage: app_drive.py <bundle> <octo> <screenshot.png> <steps.json>
# Steps (a JSON list): {"click": "<widget id or its text>"}, {"type": "<text>"} (into
# the focused input), {"key": "Return"|"Backspace"|"Escape"|...}, {"wait": <seconds>},
# {"look": true} (the widgets now, in the log).
import json, os, shutil, socket, subprocess, sys, tempfile, time, urllib.request

bundle, octo, shot, steps_file = sys.argv[1:5]
steps = json.load(open(steps_file))
work = tempfile.mkdtemp(prefix="octobuddy-app-drive.")
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
lines = []


def say(text):
    lines.append(text)


def port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def get(base, path):
    return json.loads(opener.open(base + path, timeout=15).read().decode())


def post(base, path, body):
    req = urllib.request.Request(base + path, data=json.dumps(body).encode(), headers={"content-type": "application/json"})
    return opener.open(req, timeout=20).read()


def widgets(base, q=""):
    try:
        return [w for w in get(base, "/snap?q=" + urllib.parse.quote(q)).get("s", []) if w.get("r", [0, 0, 0])[2] > 0]
    except Exception as e:
        say("(no widget list: %r)" % e)
        return []


def shown(base):
    skip = {"View", "SolidView", "RoundedView", "Window", "KeyboardView", "ScrollYView", "ScrollXView", "Splash"}
    out = []
    for w in widgets(base):
        t = (w.get("t") or "").replace(chr(10), " ")
        if w.get("ty") in skip and not t:
            continue
        out.append("  %s %s %r %s" % (w.get("ty"), w.get("i"), t[:60], w.get("r")))
    return out[:80]


import urllib.parse

try:
    shutil.copytree(bundle, os.path.join(work, "bundle"))
    p = port()
    base = "http://127.0.0.1:%d" % p
    run = subprocess.run([sys.executable, octo, "run", os.path.join(work, "bundle"), "--port", str(p), "--hidden", "--detach", "--timeout", "20"],
                         capture_output=True, text=True)
    if run.returncode != 0:
        print("the app did not start:")
        print((run.stdout + run.stderr)[-1500:])
        sys.exit(1)
    time.sleep(3)
    for i, step in enumerate(steps, 1):
        if "click" in step:
            target = str(step["click"])
            found = [w for w in widgets(base) if w.get("i") == target] or [w for w in widgets(base) if (w.get("t") or "") == target]
            if not found:
                say("%d. click %r: no widget with that id or text on screen" % (i, target))
                continue
            r = found[0]["r"]
            post(base, "/click", {"x": r[0] + r[2] // 2, "y": r[1] + r[3] // 2, "wait": 1})
            say("%d. click %r (%s at %s)" % (i, target, found[0].get("ty"), r))
        elif "type" in step:
            post(base, "/k", {"t": str(step["type"]), "wait": 1})
            say("%d. type %r" % (i, step["type"]))
        elif "key" in step:
            for k in ("down", "up"):
                post(base, "/k", {"k": k, "c": str(step["key"])})
            say("%d. key %s" % (i, step["key"]))
        elif "wait" in step:
            time.sleep(min(float(step["wait"]), 10.0))
            say("%d. wait %ss" % (i, step["wait"]))
        elif step.get("look"):
            say("%d. on screen:" % i)
            lines.extend(shown(base))
        else:
            say("%d. %r: not a step (click, type, key, wait, look)" % (i, step))
        time.sleep(0.4)
    time.sleep(0.6)
    subprocess.run([sys.executable, octo, "shot", str(p), shot], capture_output=True)
    if shutil.which("sips") and os.path.exists(shot):
        # Smaller for the agent that reads it (fewer tokens, the same widgets).
        subprocess.run(["sips", "-Z", "1280", shot], capture_output=True)
    final = shown(base)
    try:
        opener.open(base + "/quit", timeout=5)
    except Exception:
        pass
    time.sleep(1)
    print("== steps")
    print(chr(10).join(lines) if lines else "(none)")
    print("== script errors")
    log = os.path.join(work, ".local-state", "card-host.log")
    errors = []
    if os.path.exists(log):
        for l in open(log, errors="replace"):
            if "[E]" in l or "splash:" in l or "on_render closure failed" in l or "callback error" in l:
                errors.append(l.rstrip()[:300])
    print(chr(10).join(errors[:20]) if errors else "(none)")
    print("== widgets on screen at the end (type id text rect)")
    print(chr(10).join(final))
finally:
    shutil.rmtree(work, ignore_errors=True)
