"""Optional PTY smoke check; not part of make check or human terminal acceptance."""

import fcntl
import os
import pty
import select
import signal
import struct
import subprocess
import sys
import termios
import time


def run(name, stdin_tty, stdout_tty, flags=(), keys=None, want=2,
        prompt=False, result=None, resize=False):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    process = subprocess.Popen(
        [sys.argv[1], "issue-type", "list", *flags],
        stdin=slave if stdin_tty else subprocess.PIPE,
        stdout=slave if stdout_tty else subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if not stdin_tty:
        process.stdin.close()
    os.close(slave)
    screen = b""
    sent = resized = False
    deadline = time.monotonic() + 8
    try:
        while time.monotonic() < deadline:
            if select.select([master], [], [], 0.02)[0]:
                chunk = os.read(master, 65536)
                if not chunk:
                    break
                screen += chunk
            ready = b"Select project" in screen or b"Resize" in screen
            if ready and resize and not resized:
                fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 3, 10, 0, 0))
                os.kill(process.pid, signal.SIGWINCH)
                resized = True
            if ready and not sent and keys is not None:
                if not resize or b"Resize" in screen:
                    os.write(master, keys)
                    sent = True
            if process.poll() is not None:
                break
        process.wait(timeout=3)
        if stdout_tty:
            while select.select([master], [], [], 0.05)[0]:
                chunk = os.read(master, 65536)
                if not chunk:
                    break
                screen += chunk
        output = screen if stdout_tty else process.stdout.read()
        error = process.stderr.read()
        assert process.returncode == want, (name, process.returncode, error, output[-300:])
        assert (b"Select project" in screen or b"Resize" in screen) == prompt, name
        if result:
            assert result in output, (name, output[-300:])
        if want == 2:
            assert b"project is required" in error, (name, error)
        if want == 130:
            assert b"command cancelled" in error, (name, error)
        if not prompt:
            assert b"\x1b" not in output, (name, output)
        print(f"{name}: PASS exit={process.returncode} prompt={prompt}")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        os.close(master)


if len(sys.argv) != 2:
    raise SystemExit("usage: python3 terminal_check.py /absolute/path/to/terminal_mock")

run("both TTY select last project", True, True, keys=b"\x1b[F\r", want=0,
    prompt=True, result=b"project 25")
run("both TTY Ctrl-C", True, True, keys=b"\x03", want=130, prompt=True)
run("both TTY Escape", True, True, keys=b"\x1b", want=130, prompt=True)
run("stdin non-TTY", False, True)
run("stdout non-TTY", True, False)
run("both non-TTY", False, False)
run("JSON blocks UI", True, True, flags=["--json"])
run("no-input blocks UI", True, True, flags=["--no-input"])
run("false bool flags allow UI", True, True, flags=["--json=false", "--no-input=false"],
    keys=b"\r", want=0, prompt=True, result=b"project 1")
run("resize tiny then cancel", True, True, keys=b"\x03", want=130, prompt=True, resize=True)
run("key fixed output", False, False, flags=["-pAPP"], want=0, result=b"project 42")
run("direct ID JSON", False, False, flags=["-p42", "--json"], want=0,
    result=b"12345678901234567890")
