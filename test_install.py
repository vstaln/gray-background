"""Install real release bytes through Gray; Python is test-only, not runtime."""
import functools
import hashlib
import http.server
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import threading
import unittest

ROOT = Path(__file__).resolve().parent
GRAY = os.environ.get('GRAY_BACKGROUND_TEST_BINARY')
PLUGIN = Path(os.environ.get('GRAY_BACKGROUND_TEST_PLUGIN', ROOT / 'target/release/background'))
TARGETS = {('Linux', 'x86_64'): 'x86_64-unknown-linux-musl',
           ('Linux', 'aarch64'): 'aarch64-unknown-linux-musl',
           ('Darwin', 'arm64'): 'aarch64-apple-darwin',
           ('Darwin', 'x86_64'): 'x86_64-apple-darwin'}

@unittest.skipUnless(GRAY and PLUGIN.exists(), 'build release plugin and set GRAY_BACKGROUND_TEST_BINARY')
class InstallTests(unittest.TestCase):
    def test_install_autoload_repeat_corruption_and_rollback(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            releases = root / 'releases'
            releases.mkdir()
            asset = 'background-' + TARGETS[(platform.system(), platform.machine())]
            binary = releases / asset
            shutil.copyfile(PLUGIN, binary)
            digest = releases / (asset + '.sha256')
            def checksum():
                digest.write_text(hashlib.sha256(binary.read_bytes()).hexdigest() + '  ' + asset + '\n')
            checksum()
            handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(releases))
            server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), handler)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            env = {**os.environ, 'GRAY_HOME': str(root / 'home'),
                   'GRAY_BACKGROUND_RELEASE_BASE': f'http://127.0.0.1:{server.server_port}',
                   # Any accidental Python/Cargo use by the installer fails immediately.
                   'PATH': '/nonexistent'}
            def run(*args):
                return subprocess.run([GRAY, *args], env=env, cwd=root, capture_output=True,
                                      text=True, timeout=45)
            try:
                result = run('install', 'plugin', 'background')
                self.assertEqual(result.returncode, 0, result.stderr)
                lockpath = root / 'home/plugins/lock.json'
                installed = root / 'home/plugins/background/background'
                self.assertEqual(installed.read_bytes(), PLUGIN.read_bytes())
                # Exercise the installed binary's real PNG output before host ack.
                from PIL import Image
                source = root / 'input.png'
                Image.new('RGBA', (2, 3), (10, 20, 30, 128)).save(source)
                child = subprocess.Popen([str(installed)], stdin=subprocess.PIPE,
                                         stdout=subprocess.PIPE, text=True, env=env)
                try:
                    child.stdin.write(json.dumps({'id': 1, 'method': 'command/run',
                        'params': {'name': '/background', 'argv': [str(source), '0.5', 'bottom-to-top']}}) + '\n')
                    child.stdin.flush()
                    import select
                    self.assertTrue(select.select([child.stdout], [], [], 5)[0])
                    request = json.loads(child.stdout.readline())
                    prepared = Path(request['params']['path'])
                    with Image.open(prepared) as image:
                        self.assertEqual([image.getpixel((0, y))[3] for y in range(3)], [0, 32, 64])
                    child.stdin.write(json.dumps({'id': request['id'], 'result': {'ok': True}}) + '\n')
                    child.stdin.flush()
                    self.assertIn('Background set', child.stdout.readline())
                    self.assertFalse(prepared.exists())
                finally:
                    child.stdin.close()
                    child.wait(timeout=5)
                    child.stdout.close()
                registry = json.loads(lockpath.read_text())
                self.assertTrue(registry['plugins']['background']['enabled'])
                result = run('--dump-manifest')
                self.assertEqual(result.returncode, 0, result.stderr)
                manifests = json.loads(result.stdout)
                self.assertIn('background', [m['name'] for m in manifests])
                self.assertTrue(any(t['name'] == 'bash' for m in manifests for t in m['tools']),
                                'installing a sidecar must not remove default bash')
                original = lockpath.read_bytes()
                result = run('install', 'plugin', 'background')
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(lockpath.read_bytes(), original)
                registry['plugins']['background']['enabled'] = False
                lockpath.write_text(json.dumps(registry))
                original = lockpath.read_bytes()
                digest.write_text('0' * 64 + '  ' + asset + '\n')
                result = run('install', 'plugin', 'background')
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('hash mismatch', result.stderr)
                self.assertEqual(lockpath.read_bytes(), original)
                self.assertEqual(installed.read_bytes(), PLUGIN.read_bytes())
                binary.write_bytes(b'not an executable')
                checksum()
                result = run('install', 'plugin', 'background')
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(lockpath.read_bytes(), original)
                self.assertEqual(installed.read_bytes(), PLUGIN.read_bytes())
                shutil.copyfile(PLUGIN, binary)
                checksum()
                result = run('install', 'plugin', 'background')
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertTrue(json.loads(lockpath.read_text())['plugins']['background']['enabled'])
            finally:
                server.shutdown()
                server.server_close()
                thread.join()

if __name__ == '__main__':
    unittest.main()
