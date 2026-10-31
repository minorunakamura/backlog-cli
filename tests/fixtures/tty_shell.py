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
    height, width = struct.unpack("HHHH", fcntl.ioctl(slave, termios.TIOCGWINSZ, b"\0" * 8))[:2]
    # shortcut: cursor-addressed shell output only; use an emulator for more VT behavior.
    cells = [[" "] * width for _ in range(height)]
    row = col = 0
    for token in re.split(r"(\x1b\[[0-?]*[ -/]*[@-~])", data):
        if token.startswith("\x1b["):
            if token.endswith("H"):
                coordinates = token[2:-1].split(";")
                row, col = [int(n or 1) - 1 for n in coordinates]
            elif token in ("\x1b[2J", "\x1b[3J"):
                cells = [[" "] * width for _ in range(height)]
            continue
        for char in token:
            if char == "\r":
                col = 0
            elif char == "\n":
                row += 1
            elif not unicodedata.combining(char):
                if 0 <= row < height and 0 <= col < width:
                    cells[row][col] = char
                col += 2 if unicodedata.east_asian_width(char) in ("W", "F") else 1
    return "\n".join("".join(row) for row in cells)

def report_failure(failure):
    requests = []
    ordinals = {}
    try:
        with open(os.environ["SHELL_REQUESTS_FILE"]) as source:
            for line in source:
                try:
                    record = json.loads(line)
                    method, path = record["method"], record["path"].split("?", 1)[0]
                    key = (method, path)
                    ordinals[key] = ordinals.get(key, 0) + 1
                    requests.append({"method": method, "path": path, "ordinal": ordinals[key]})
                except (ValueError, KeyError, TypeError):
                    # A concurrent request may still be writing its final line.
                    continue
    except (OSError, KeyError):
        pass
    # Allowlist metadata only: no exception message, steps, screens, or raw HTTP records.
    print(json.dumps({"failure": failure, "step_index": step_index,
                      "elapsed": time.monotonic() - started, "sent_at": sent_at[:step_index],
                      "request_counts": request_counts[:step_index],
                      "request_count": len(requests), "requests": requests,
                      "child_pid": child.pid}), file=sys.stderr, flush=True)


mode, binary, *args = sys.argv[1:]
deadline_seconds = int(os.environ.get("SHELL_DEADLINE_SECONDS", "8"))
assert deadline_seconds > 0, "shell scenario deadline must be positive"
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
steps_path = os.environ.get("SHELL_STEPS_FILE")
snapshots = []
sent_at = []
request_counts = []
browse_request_counts = []
step_ready_at = None
step_index = 0
try:
    steps = json.load(open(steps_path)) if steps_path and os.path.exists(steps_path) else []
    while child.poll() is None:
        if time.monotonic() - started > deadline_seconds:
            report_failure("deadline")
            raise SystemExit(1)
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
        if mode == "scripted" and steps:
            step = steps[0]
            screen = screen_text(output.decode("utf-8", "replace"))
            marker = step.get("wait_file")
            marker_ready = not marker or os.path.exists(os.path.join(os.path.dirname(os.environ["SHELL_REQUESTS_FILE"]), marker))
            if step.get("wait", "") in screen and marker_ready:
                if step_ready_at is None:
                    step_ready_at = time.monotonic() + step.get("pause", 0.08)
                if time.monotonic() >= step_ready_at:
                    snapshots.append(screen)
                    requests_path = os.environ.get("SHELL_REQUESTS_FILE")
                    request_counts.append(len(open(requests_path).readlines()) if requests_path and os.path.exists(requests_path) else 0)
                    records = [json.loads(line) for line in open(requests_path)] if requests_path and os.path.exists(requests_path) else []
                    browse_request_counts.append(sum(not re.fullmatch(r"/api/v2/(issues/[^/?]+(?:/comments\?.*)?|projects/[^/?]+)", record["path"]) for record in records))
                    sent_at.append(time.time())
                    if "resize" in step:
                        height, width = step["resize"]
                        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))
                        child.send_signal(signal.SIGWINCH)
                    if "remove" in step:
                        os.unlink(step["remove"])
                    if "signal" in step:
                        child.send_signal(getattr(signal, step["signal"]))
                    os.write(master, step.get("keys", "").encode())
                    steps.pop(0)
                    step_index += 1
                    step_ready_at = None
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
    print(json.dumps({"code": child.returncode, "snapshots": snapshots, "sent_at": sent_at, "request_counts": request_counts, "browse_request_counts": browse_request_counts,
                      "stdout": output.decode("utf-8", "replace"),
                      "screen": screen_text(output.decode("utf-8", "replace")),
                      "stderr": stderr.decode("utf-8", "replace"),
                      "restored": after == before,
                      "termios_diff": [[i, str(a), str(b)] for i, (a, b) in enumerate(zip(before, after)) if a != b],
                      "elapsed": time.monotonic() - started}))
except Exception as error:
    report_failure(type(error).__name__)
    raise SystemExit(1)
finally:
    if child.poll() is None:
        child.kill()
        child.wait()
    os.close(master)
    os.close(slave)
