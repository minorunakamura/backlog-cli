"""Blocking editor at the process/TTY boundary, never used by the product."""
import json
import os
import pathlib
import signal
import sys
import termios
import time
import tty

record, mode, target = sys.argv[1:]
p = pathlib.Path(target)
state = {"path": str(p), "pid": os.getpid(), "mode": p.stat().st_mode & 0o777,
         "uid": p.stat().st_uid, "original": p.read_bytes().decode("utf-8"),
         "cooked": bool(termios.tcgetattr(0)[3] & termios.ICANON),
         "echo": bool(termios.tcgetattr(0)[3] & termios.ECHO)}
if mode.startswith("raw-"):
    mode = mode[4:]
    tty.setraw(0)
    state["raw_noecho"] = not bool(termios.tcgetattr(0)[3] & (termios.ICANON | termios.ECHO))
pathlib.Path(record).write_text(json.dumps(state))
print("\x1b[2J\x1b[1;1HEDITOR READY", flush=True)
state["input"] = sys.stdin.readline()
pathlib.Path(record).write_text(json.dumps(state))
if mode == "signal":
    os.kill(os.getpid(), signal.SIGTERM)
elif mode == "interrupt":
    time.sleep(20)
elif mode == "utf8":
    p.write_bytes(b"\xff")
elif mode == "unlink":
    p.unlink()
elif mode == "body":
    p.write_text(" 編集後\n\n")
else:
    p.write_text('{"saved":true}')
if mode == "nonzero":
    sys.exit(7)
