"""Run a multi-step flow through one real controlling terminal."""

import errno
import fcntl
import json
import os
import pty
import select
import struct
import subprocess
import sys
import tempfile
import termios
import time
import unittest


BINARY = os.path.abspath(sys.argv.pop(1))


def interact(document, interactions):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    original = termios.tcgetattr(slave)
    process = None

    def attach_terminal():
        os.setsid()
        fcntl.ioctl(2, termios.TIOCSCTTY, 0)

    try:
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json") as file:
            json.dump(document, file)
            file.flush()
            process = subprocess.Popen(
                [BINARY, "flow", file.name],
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=slave,
                preexec_fn=attach_terminal,
                env={**os.environ, "TERM": "xterm-256color"},
            )
            screen = bytearray()
            deadline = time.monotonic() + 10
            cursor_queries = 0
            interaction = 0
            while time.monotonic() < deadline:
                if select.select([master], [], [], 0.1)[0]:
                    try:
                        screen.extend(os.read(master, 65536))
                    except OSError as error:
                        if error.errno != errno.EIO:
                            raise
                pending_queries = screen.count(b"\x1b[6n")
                if pending_queries > cursor_queries:
                    os.write(master, b"\x1b[8;1R" * (pending_queries - cursor_queries))
                    cursor_queries = pending_queries
                if interaction < len(interactions):
                    needle, keys = interactions[interaction]
                    if needle in screen:
                        os.write(master, keys)
                        interaction += 1
                        screen.clear()
                        cursor_queries = 0
                if process.poll() is not None:
                    while select.select([master], [], [], 0)[0]:
                        screen.extend(os.read(master, 65536))
                    break
            assert interaction == len(interactions), repr(bytes(screen))
            assert process.poll() is not None, "flow did not exit"
            assert termios.tcgetattr(slave) == original, "terminal settings were not restored"
            return process.returncode, process.stdout.read(), bytes(screen)
    finally:
        if process is not None:
            if process.poll() is None:
                process.kill()
            process.wait()
            process.stdout.close()
        os.close(slave)
        os.close(master)


class FlowTerminalTest(unittest.TestCase):
    def test_input_value_interpolates_into_following_confirmation(self):
        document = {
            "version": 1,
            "name": "interactive release",
            "variables": {"environment": "production"},
            "steps": [
                {
                    "id": "release",
                    "component": "input",
                    "save_as": "release",
                    "args": {
                        "title": "Release name",
                        "required": True,
                        "theme": "nord",
                    },
                },
                {
                    "id": "approval",
                    "component": "confirm",
                    "args": {
                        "message": "Deploy ${variables.release.value} to ${variables.environment}?",
                        "theme": "dracula",
                    },
                },
            ],
        }
        code, value, screen = interact(
            document,
            [(b"Release name", b"v3.0.0\r"), (b"Deploy v3.0.0 to production?", b"y")],
        )
        self.assertEqual(code, 0)
        report = json.loads(value)
        self.assertEqual(report["status"], "completed")
        self.assertEqual(report["variables"]["release"]["value"], "v3.0.0")
        self.assertEqual(report["steps"]["approval"]["confirmed"], True)
        self.assertNotIn(b"\x1b[?1049h", screen)
        self.assertIn(b"\x1b[?2004l", screen)


if __name__ == "__main__":
    unittest.main()
