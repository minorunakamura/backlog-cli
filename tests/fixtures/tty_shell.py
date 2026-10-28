"""Drive the real shell through a PTY; report output and terminal restoration."""
import fcntl
import json
import os
import pty
import re
import select
import signal
import struct
import subprocess
import sys
import termios
import time
import unicodedata


def screen_text(data):
    # shortcut: cursor-addressed shell output only; use an emulator for more VT behavior.
    cells = [[" "] * 100 for _ in range(24)]
    row = col = 0
    for token in re.split(r"(\x1b\[[0-?]*[ -/]*[@-~])", data):
        if token.startswith("\x1b["):
            if token.endswith("H"):
                coordinates = token[2:-1].split(";")
                row, col = [int(n or 1) - 1 for n in coordinates]
            elif token in ("\x1b[2J", "\x1b[3J"):
                cells = [[" "] * 100 for _ in range(24)]
            continue
        for char in token:
            if char == "\r":
                col = 0
            elif char == "\n":
                row += 1
            elif not unicodedata.combining(char):
                if 0 <= row < 24 and 0 <= col < 100:
                    cells[row][col] = char
                col += 2 if unicodedata.east_asian_width(char) in ("W", "F") else 1
    return "\n".join("".join(row) for row in cells)

mode, binary, *args = sys.argv[1:]
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 100, 0, 0))
before = termios.tcgetattr(slave)
output = bytearray()
started = time.monotonic()
child = subprocess.Popen([binary, *args],
                         stdin=subprocess.DEVNULL if mode == "stdout-only" else slave,
                         stdout=subprocess.PIPE if mode == "stdin-only" else slave,
                         stderr=subprocess.PIPE)
action_sent = False
paste_sent = False
input_left = False
try:
    while child.poll() is None:
        if time.monotonic() - started > 8:
            raise AssertionError("shell did not exit responsively")
        if select.select([master], [], [], 0.02)[0]:
            output.extend(os.read(master, 65536))
        if not action_sent and mode == "early-signal" and b"\x1b[?25l" in output:
            child.send_signal(signal.SIGINT)
            os.write(master, b"\x00")
            action_sent = True
        if mode.startswith("input-"):
            if not paste_sent and b"Input" in output:
                os.write(master, b"qhljk\x1b[200~test-secret\r\n\t\x1b]52;c;clipboard\x07\x1b[201~")
                paste_sent = True
            if not input_left and b"[REDACTED]" in output:
                os.write(master, b"\x03" if mode == "input-interrupt" else b"\x1b")
                input_left = True
                action_sent = mode == "input-interrupt"
            if not action_sent and b"unfocused" in output:
                os.write(master, b"q")
                action_sent = True
        # Other actions wait for a rendered screen, not merely raw-mode entry.
        marker = os.environ.get("SHELL_WAIT_MARKER")
        ready = not marker or os.path.exists(marker)
        if not action_sent and b"Project List" in output and ready:
            if mode in ("quit", "interrupt", "signal"):
                if mode == "signal":
                    child.send_signal(signal.SIGINT)
                else:
                    os.write(master, b"q" if mode == "quit" else b"\x03")
                action_sent = True
            elif mode == "loaded" and b"Actual space" in output:
                os.write(master, b"q")
                action_sent = True
    stdout, stderr = child.communicate(timeout=1)
    while select.select([master], [], [], 0.05)[0]:
        output.extend(os.read(master, 65536))
    if stdout:
        output.extend(stdout)
    after = termios.tcgetattr(slave)
    # macOS sets transient PENDIN when restoring cooked input without a subsequent read.
    before[3] &= ~getattr(termios, "PENDIN", 0)
    after[3] &= ~getattr(termios, "PENDIN", 0)
    print(json.dumps({"code": child.returncode,
                      "stdout": output.decode("utf-8", "replace"),
                      "screen": screen_text(output.decode("utf-8", "replace")),
                      "stderr": stderr.decode("utf-8", "replace"),
                      "restored": after == before,
                      "termios_diff": [[i, str(a), str(b)] for i, (a, b) in enumerate(zip(before, after)) if a != b],
                      "elapsed": time.monotonic() - started}))
finally:
    if child.poll() is None:
        child.kill()
        child.wait()
    os.close(master)
    os.close(slave)
