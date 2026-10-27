"""Run the real CLI with terminal stdin/stdout, without providing any input."""
import os
import pty
import subprocess
import sys
import tempfile
import threading

master, slave = pty.openpty()
output = bytearray()


def read_terminal():
    while True:
        try:
            chunk = os.read(master, 4096)
        except OSError:
            break
        if not chunk:
            break
        output.extend(chunk)


reader = threading.Thread(target=read_terminal, daemon=True)
reader.start()
with tempfile.TemporaryFile() as errors:
    child = subprocess.Popen(sys.argv[1:], stdin=slave, stdout=slave, stderr=errors)
    os.close(slave)
    try:
        code = child.wait(timeout=10)
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait()
        raise
    reader.join(timeout=1)
    os.close(master)
    errors.seek(0)
    sys.stderr.buffer.write(errors.read())
    sys.stdout.buffer.write(output)
    sys.exit(code)
