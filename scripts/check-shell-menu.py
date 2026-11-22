#!/usr/bin/env python3
"""The fili-bar menu of Conversations (shell/appmenu/fili-im.json) stays in agreement with the window (qml/Main.qml): every
`fili-im ctl --pid %pid action NAME` of the menu is an action of the window, every action of the window is in the menu (the few
that have none are listed below), every announced shortcut is a key sequence of the window's QML, and the brief of Lia
(shell/lia/fili-im.md) exists. Usage: scripts/check-shell-menu.py (from anywhere)."""
import json, pathlib, re, sys

root = pathlib.Path(__file__).resolve().parents[1]
menu = json.loads((root / "shell/appmenu/fili-im.json").read_text())
qml = (root / "qml/Main.qml").read_text()
body = qml[qml.index("function onCommand(name)"):]
actions = set(re.findall(r'name === "([a-z-]+)"', body))
actions |= {"status-" + s for s in re.findall(r'rpc\("presence\.set"', body) and ["available", "away", "busy", "offline"]}
sequences = set()
for f in (root / "qml").glob("*.qml"):
    for m in re.finditer(r'sequences?: (\[[^\]]*\]|"[^"]*")', f.read_text()):
        sequences |= set(re.findall(r'"([^"]+)"', m.group(1)))
# Test hooks for the attention colours (ctl only), and Quick Look of the last attachment (reached from the message itself)
without_menu = {"attention", "attention-clear", "quick-look"}
# Keys the text fields handle themselves (copy and select all), not a Shortcut of the window
native_keys = {"Ctrl+C", "Ctrl+A"}
errors = []
if not (root / "shell/lia/fili-im.md").is_file():
    errors.append("brief of Lia missing: shell/lia/fili-im.md")

def entries(items):
    for e in items:
        if "children" in e:
            yield from entries(e["children"])
        elif not e.get("sep"):
            yield e

everything = list(entries(menu["app"] + menu["window"] + [m for top in menu["menus"] for m in top["children"]]))
used = set()
for e in everything:
    cmd = e.get("exec", "")
    if not cmd.startswith("fili-im ctl"):
        continue  # a command that opens a file (Settings…)
    m = re.fullmatch(r"fili-im ctl --pid %pid action ([a-z-]+)", cmd)
    if not m or m.group(1) not in actions:
        errors.append(f"command without an action in Main.qml: {cmd}")
    else:
        used.add(m.group(1))
    k = e.get("keys")
    if k and k not in sequences | native_keys:
        errors.append(f"shortcut without a key sequence in the window: {k}")
for a in sorted(actions - used - without_menu):
    errors.append(f"action of Main.qml missing from the menu: {a}")
if errors:
    print("not ok - menu of Conversations:")
    print("\n".join("#   " + e for e in errors))
    sys.exit(1)
print("ok - menu of Conversations agrees with Main.qml (%d actions)" % len(actions))
