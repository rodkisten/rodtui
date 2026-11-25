"""Exercise confirm and input against a real controlling terminal."""

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


def interact(arguments, ready_text, keys):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    original = termios.tcgetattr(slave)
    process = None

    def attach_terminal():
        os.setsid()
        fcntl.ioctl(2, termios.TIOCSCTTY, 0)

    try:
        process = subprocess.Popen(
            [BINARY, *arguments],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=slave,
            preexec_fn=attach_terminal,
            env={**os.environ, "TERM": "xterm-256color"},
        )
        screen = bytearray()
        deadline = time.monotonic() + 8
        sent = False
        cursor_queries = 0
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
            if not sent and ready_text in screen:
                os.write(master, keys)
                sent = True
            if process.poll() is not None:
                while select.select([master], [], [], 0)[0]:
                    screen.extend(os.read(master, 65536))
                break
        assert sent, repr(bytes(screen))
        assert process.poll() is not None, "component did not exit"
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


class FormsTerminalTest(unittest.TestCase):
    def test_confirm_negative_choice_has_exit_one_and_json(self):
        code, value, screen = interact(
            ["confirm", "Deploy production?", "--output", "json", "--theme", "dracula"],
            b"Deploy production?",
            b"\x1b[C\r",
        )
        self.assertEqual(code, 1)
        self.assertEqual(json.loads(value), {"confirmed": False})
        self.assertNotIn(b"\x1b[?1049h", screen)
        self.assertIn(b"\x1b[?2004l", screen)

    def test_input_keeps_editing_after_validation_error(self):
        code, value, screen = interact(
            ["input", "--required", "--min-length", "4", "--output", "json"],
            b"Input",
            "ab\rçd\r".encode(),
        )
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(value), {"value": "abçd"})
        self.assertIn(b"\x1b[?2004l", screen)

    def test_fullscreen_input_cancel_restores_terminal(self):
        code, value, screen = interact(
            ["input", "--fullscreen", "--theme", "nord"],
            b"Input",
            b"\x03",
        )
        self.assertEqual((code, value), (130, b""))
        self.assertIn(b"\x1b[?1049h", screen)
        self.assertIn(b"\x1b[?1049l", screen)
        self.assertIn(b"\x1b[?2004l", screen)

    def test_declarative_confirm_uses_the_same_interactive_terminal(self):
        document = {
            "version": 1,
            "component": "confirm",
            "args": {
                "message": "Ship declarative release?",
                "theme": "gruvbox",
                "output": "json",
            },
        }
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json") as file:
            json.dump(document, file)
            file.flush()
            code, value, screen = interact(
                ["run", file.name], b"Ship declarative release?", b"y"
            )
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(value), {"confirmed": True})
        self.assertIn(b"\x1b[?2004l", screen)


if __name__ == "__main__":
    unittest.main()
