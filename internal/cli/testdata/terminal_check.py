"""Optional PTY smoke check; not part of make check or human terminal acceptance."""

import fcntl
import os
import pty
import json
import re
import select
import signal
import struct
import subprocess
import sys
import termios
import time


def run(name, stdin_tty, stdout_tty, flags=(), keys=None, want=2,
        prompt=False, result=None, resize=False, command=("issue-type", "list"),
        env=None, viewer=False, cancel_signal=False, widths=()):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    process = subprocess.Popen(
        [sys.argv[1], *command, *flags],
        env={**os.environ, **(env or {})},
        stdin=slave if stdin_tty else subprocess.PIPE,
        stdout=slave if stdout_tty else subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if not stdin_tty:
        process.stdin.close()
    os.close(slave)
    screen = b""
    sent = resized = False
    ready_at = None
    width_index = 0
    width_changed_at = 0
    width_screen_size = 0
    deadline = time.monotonic() + 8
    try:
        while time.monotonic() < deadline:
            if select.select([master], [], [], 0.02)[0]:
                chunk = os.read(master, 65536)
                if not chunk:
                    break
                screen += chunk
            ready = b"Select project" in screen or b"Resize" in screen or b"q close" in screen
            if ready and ready_at is None:
                ready_at = time.monotonic()
            if viewer and ready and time.monotonic() - ready_at < 0.3:
                assert process.poll() is None, (name, "viewer did not wait")
                continue
            if ready and widths:
                if time.monotonic() - width_changed_at < 0.3:
                    continue
                if width_index:
                    assert len(screen) > width_screen_size, (name, "no resize redraw", widths[width_index - 1])
                if width_index < len(widths):
                    fcntl.ioctl(master, termios.TIOCSWINSZ,
                                struct.pack("HHHH", 24, widths[width_index], 0, 0))
                    os.kill(process.pid, signal.SIGWINCH)
                    width_index += 1
                    width_changed_at = time.monotonic()
                    width_screen_size = len(screen)
                    continue
            if ready and resize and not resized:
                fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 3, 10, 0, 0))
                os.kill(process.pid, signal.SIGWINCH)
                resized = True
            if ready and not sent and keys is not None:
                if not resize or b"Resize" in screen:
                    if cancel_signal:
                        os.kill(process.pid, signal.SIGINT)
                    else:
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
        assert (b"Select project" in screen or b"Resize" in screen or b"q close" in screen) == prompt, name
        if result:
            plain = re.sub(rb"\x1b\[[0-9;]*m", b"", output)
            assert result in plain, (name, plain[-300:])
        if want == 2:
            assert b"project is required" in error, (name, error)
        if want == 130:
            assert b"command cancelled" in error, (name, error)
        if not prompt and not (stdout_tty and command[0] == "issue" and "--json" not in flags):
            assert b"\x1b" not in output, (name, output)
        if command[0] == "issue":
            assert b"mock-only-secret" not in output, name
            if "--json" in flags:
                original = json.loads(output)["description"]
                assert "&#27;" in original and "\x1b]52;" in original, (name, original)
            else:
                assert b"stolen" not in output, name
                if env and env.get("BL_TERMINAL_MOCK_BODY") == "styled-secret":
                    plain = re.sub(rb"\x1b\[[0-9;]*m", b"", output)
                    assert b"mock-" not in plain and b"only-secret" not in plain, (name, plain)
                    assert b"[REDACTED]" in plain, (name, plain)
                    assert width_index == len(widths), (name, width_index)
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

run("short detail waits for q", True, True, command=("issue", "view", "APP-1"),
    env={"BL_TERMINAL_MOCK_BODY": "short"}, viewer=True,
    keys=b"q", want=0, prompt=True, result="短い本文 日本語👩‍💻".encode())

for rule in ("markdown", "backlog"):
    command = ("issue", "view", "APP-1")
    env = {"BL_TERMINAL_MOCK_RULE": rule}
    for name, input_tty, output_tty, flags in (
        ("stdin non-TTY", False, True, ()),
        ("stdout non-TTY", True, False, ()),
        ("both non-TTY", False, False, ()),
        ("JSON", True, True, ("--json",)),
        ("no-input", True, True, ("--no-input",)),
    ):
        run(f"{rule} detail {name}", input_tty, output_tty, flags=flags,
            command=command, env=env, want=0, result=b"APP-1")
    run(f"{rule} short detail false flags wait for q", True, True,
        command=command, env={**env, "BL_TERMINAL_MOCK_BODY": "short"},
        flags=("--json=false", "--no-input=false"), viewer=True,
        keys=b"q", want=0, prompt=True)
    run(f"{rule} long detail tiny resize", True, True,
        command=command, env={**env, "BL_TERMINAL_MOCK_BODY": "long"},
        viewer=True, keys=b"q", want=0, prompt=True, resize=True)
    run(f"{rule} styled secret width resize", True, True,
        command=command, env={**env, "BL_TERMINAL_MOCK_BODY": "styled-secret"},
        viewer=True, keys=b"q", want=0, prompt=True, widths=(20, 36, 80, 20, 80))
    run(f"{rule} detail Ctrl-C", True, True, command=command, env=env,
        viewer=True, keys=b"\x03", want=130, prompt=True)
    run(f"{rule} detail context cancel", True, True, command=command, env=env,
        viewer=True, keys=b"", want=130, prompt=True, cancel_signal=True)


def detail_navigation(rule):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    process = subprocess.Popen(
        [sys.argv[1], "issue", "view", "APP-1"], stdin=slave, stdout=slave,
        stderr=subprocess.PIPE, env={**os.environ, "BL_TERMINAL_MOCK_RULE": rule,
                                    "BL_TERMINAL_MOCK_BODY": "long"})
    os.close(slave)

    def observe(needle=None):
        screen = b""
        deadline = time.monotonic() + (4 if needle else 0.3)
        while time.monotonic() < deadline:
            if select.select([master], [], [], 0.02)[0]:
                screen += os.read(master, 65536)
            if needle and needle in screen:
                return screen
        assert needle is None, (rule, needle, screen[-500:])
        return screen

    try:
        observe(b"q close")
        for keys in (b"j", b"k", b"\x1b[B", b"\x1b[A", b"\x1b[6~",
                     b"\x1b[5~", b" ", b"b"):
            os.write(master, keys)
            assert observe(), (rule, "no navigation redraw", keys)
        for keys, needle in ((b"G", b"099"), (b"g", b"APP-1"),
                             (b"\x1b[F", b"099"), (b"\x1b[H", b"APP-1")):
            os.write(master, keys)
            observe(needle)
        fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 10, 36, 0, 0))
        os.kill(process.pid, signal.SIGWINCH)
        narrow = observe(b"PgUp/PgDn")
        assert "日本語".encode() in narrow, (rule, narrow)
        fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        os.kill(process.pid, signal.SIGWINCH)
        observe(b"q close")
        os.write(master, b"q")
        assert process.wait(timeout=4) == 0, process.stderr.read()
        print(f"{rule} detail navigation and width resize: PASS exit=0")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        os.close(master)


for rule in ("markdown", "backlog"):
    detail_navigation(rule)
