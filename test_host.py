"""Real Gray TUI + plugin integration under a PTY; no display or provider calls.

GRAY_BACKGROUND_TEST_BINARY=/absolute/path/to/gray python3 -m unittest -v test_host
This checks the emitted protocol, NOT visual rendering in a terminal emulator.
"""
import os
import json
from pathlib import Path
import pty
import select
import signal
import tempfile
import termios
import time
import unittest

ROOT = Path(__file__).resolve().parent
BINARY = os.environ.get('GRAY_BACKGROUND_TEST_BINARY')
PLUGIN = Path(os.environ.get('GRAY_BACKGROUND_TEST_PLUGIN', ROOT / 'target/release/background'))


@unittest.skipUnless(BINARY, 'set GRAY_BACKGROUND_TEST_BINARY to test a rebuilt Gray')
class HostTests(unittest.TestCase):
    def test_first_command_upload_resize_off_and_quit(self):
        from PIL import Image
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            Image.new('RGB', (2, 2), 'red').save(root / 'red.png')
            plugins = root / 'home/plugins'
            plugins.mkdir(parents=True)
            (plugins / 'lock.json').write_text(json.dumps({'schema': 1, 'plugins': {'background': {
                'ecosystem': 'gray-native', 'version': '0.1.0', 'hash': '', 'source': '',
                'argv': [str(PLUGIN)], 'adapter_version': '1.1', 'installed_at': '',
                'scope': 'user', 'enabled': True}}}))
            pid, master = pty.fork()
            if pid == 0:
                os.chdir(root)
                os.environ['GRAY_HOME'] = str(root / 'home')
                os.environ['TERM_PROGRAM'] = 'ghostty'
                os.environ['TERM'] = 'xterm-256color'
                os.environ.pop('TMUX', None)
                os.environ.pop('STY', None)
                os.execv(BINARY, [BINARY, '--model', 'test/local', '--base-url',
                                 'http://127.0.0.1:1/v1', '--context-window', '32000'])
            termios.tcsetwinsize(master, (24, 80))
            output = bytearray()
            def wait_for(needle, start=0):
                deadline = time.monotonic() + 15
                while time.monotonic() < deadline:
                    if needle in output[start:]:
                        return
                    if select.select([master], [], [], 0.1)[0]:
                        try:
                            data = os.read(master, 65536)
                        except OSError:
                            break
                        output.extend(data)
                        # Crossterm's cursor probe; no fabricated graphics responses.
                        if b'\x1b[6n' in data:
                            os.write(master, b'\x1b[1;1R')
                self.fail(f'missing {needle!r}; terminal tail: {bytes(output[-2000:])!r}')
            def command(text):
                start = len(output)
                os.write(master, text.encode() + b'\r')
                return start
            try:
                wait_for(b'\x1b[?25h')
                start = command(f'/background {root / "red.png"} 0.5')
                wait_for(b'a=t,f=100', start)
                wait_for(b'c=80,r=24,z=-1,C=1', start)
                self.assertIn(b'z=0,C=1', output)
                start = command('/help')
                wait_for(b'c=80,r=24,z=-1,C=1', start)
                start = len(output)
                termios.tcsetwinsize(master, (30, 100))
                os.kill(pid, signal.SIGWINCH)
                wait_for(b'c=100,r=30,z=-1,C=1', start)
                start = command('/plugin')
                wait_for(b'a=d,d=I', start)
                wait_for(b'\x1b[?1049h', start)
                start = len(output)
                os.write(master, b'\x1b')
                wait_for(b'c=100,r=30,z=-1,C=1', start)
                start = command('/background off')
                wait_for(b'a=d,d=I', start)
                start = command(f'/background {root / "red.png"} 0.5')
                wait_for(b'a=t,f=100', start)
                start = command('/quit')
                wait_for(b'a=d,d=I', start)
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    done, status = os.waitpid(pid, os.WNOHANG)
                    if done:
                        self.assertEqual(os.waitstatus_to_exitcode(status), 0)
                        pid = None
                        break
                    time.sleep(0.05)
                self.assertIsNone(pid, 'Gray did not exit')
            finally:
                if pid is not None:
                    os.kill(pid, signal.SIGKILL)
                    os.waitpid(pid, 0)
                os.close(master)


if __name__ == '__main__':
    unittest.main()
