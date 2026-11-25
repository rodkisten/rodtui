"""Exercise the compiled chooser with piped input and a controlling terminal."""

import errno
import fcntl
import os
import pty
import select
import struct
import subprocess
import sys
import termios
import time
import unittest


BINARY = os.path.abspath(sys.argv.pop(1))


def interact(arguments, candidates, keys):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    original = termios.tcgetattr(slave)
    process = None

    def attach_terminal():
        os.setsid()
        fcntl.ioctl(2, termios.TIOCSCTTY, 0)

    try:
        process = subprocess.Popen(
            [BINARY, "choose", *arguments],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=slave,
            preexec_fn=attach_terminal,
            env={**os.environ, "TERM": "xterm-256color"},
        )
        process.stdin.write(candidates)
        process.stdin.close()
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
            if not sent and b"Deploy" in screen:
                os.write(master, keys)
                sent = True
            if process.poll() is not None:
                while select.select([master], [], [], 0)[0]:
                    screen.extend(os.read(master, 65536))
                break
        assert sent, repr(bytes(screen))
        assert process.poll() is not None, "chooser did not exit"
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


class ChooseTerminalTest(unittest.TestCase):
    def test_inline_multiselect_with_piped_input(self):
        code, value, screen = interact(
            ["--multi", "--theme", "gruvbox"], b"Deploy\nBuild\nTest\n", b"\t\t\r"
        )
        self.assertEqual((code, value), (0, b"Deploy\nBuild\n"))
        self.assertNotIn(b"\x1b[?1049h", screen)
        self.assertIn(b"\x1b[?2004l", screen)

    def test_fullscreen_cancellation_restores_terminal(self):
        code, value, screen = interact(
            ["--fullscreen", "--theme", "nord"], b"Deploy\nBuild\n", b"\x03"
        )
        self.assertEqual((code, value), (130, b""))
        self.assertIn(b"\x1b[?1049h", screen)
        self.assertIn(b"\x1b[?1049l", screen)
        self.assertIn(b"\x1b[?2004l", screen)

    def test_filter_with_space_in_multiselect(self):
        code, value, _ = interact(
            ["--filter", "--multi", "--theme", "dracula"],
            b"Deploy staging\nDeploy production\nBuild\n",
            b"deploy production\t\r",
        )
        self.assertEqual((code, value), (0, b"Deploy production\n"))


if __name__ == "__main__":
    unittest.main()
