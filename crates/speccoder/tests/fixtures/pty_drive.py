"""Drives a program in a pseudo-terminal, as a person at a keyboard would.

Usage: pty_drive.py <cols> <rows> <step>... -- <program> <args>...

Each step is `wait:<text>` (until <text> is on the screen, escape codes
removed; 60 s at most) or `send:<keys>`. Prints one JSON object: whether
every wait was met, the exit code, and everything the program wrote, with
escape sequences removed.
"""
import fcntl
import json
import os
import pty
import re
import select
import struct
import sys
import termios
import time

cols, rows = int(sys.argv[1]), int(sys.argv[2])
split = sys.argv.index("--")
steps, program = sys.argv[3:split], sys.argv[split + 1:]

pid, fd = pty.fork()
if pid == 0:
    os.environ["TERM"] = "xterm-256color"
    os.execv(program[0], program)
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))

ESCAPES = re.compile(rb"\x1b\[[0-9;?]*[A-Za-z]|\x1b\][^\x07]*\x07|\x1b[()][A-Z0-9]|\x1b[=>]")
out = b""
met = []


def read(seconds):
    global out
    r, _, _ = select.select([fd], [], [], seconds)
    if not r:
        return True
    try:
        data = os.read(fd, 65536)
    except OSError:
        return False
    out += data
    return bool(data)


for step in steps:
    kind, _, arg = step.partition(":")
    if kind == "send":
        os.write(fd, arg.encode())
        continue
    end, ok = time.time() + 60, False
    while time.time() < end:
        if arg.encode() in ESCAPES.sub(b"", out):
            ok = True
            break
        if not read(0.2):
            break
    met.append(ok)

while read(0.5):
    pass
_, status = os.waitpid(pid, 0)
print(json.dumps({
    "met": met,
    "exit": os.waitstatus_to_exitcode(status),
    "text": ESCAPES.sub(b"", out).decode(errors="replace"),
}))
