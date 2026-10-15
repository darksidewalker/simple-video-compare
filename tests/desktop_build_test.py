"""Desktop packaging must embed React and rebuild rather than ship a stale shell."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]

class DesktopBuildTests(unittest.TestCase):
    def test_desktop_embeds_local_frontend(self):
        config = json.loads((ROOT / 'src-tauri/tauri.conf.json').read_text())
        self.assertEqual(config['build']['frontendDist'], '../desktop/dist')
        self.assertEqual(config['build']['beforeBuildCommand'], 'npm --prefix ../desktop run build')
        self.assertNotIn('devUrl', config['build'])
        main = (ROOT / 'src-tauri/src/main.rs').read_text()
        self.assertIn('native::native_open', main)
        self.assertNotIn('go_binary_path', main)
        self.assertNotIn('install_media_plugins', main)
        self.assertNotIn('macOS', (ROOT / 'desktop/src/App.tsx').read_text())

    def test_native_controls_have_minimal_local_permissions(self):
        capability = json.loads((ROOT / 'src-tauri/capabilities/desktop.json').read_text())
        self.assertEqual(capability['windows'], ['main'])
        self.assertNotIn('remote', capability)
        self.assertEqual(set(capability['permissions']), {
            'core:event:allow-listen', 'core:event:allow-unlisten',
            'core:window:allow-scale-factor', 'core:window:allow-close',
        })

    def test_shell_scripts_build_react_and_do_not_skip_existing_binary(self):
        shell = (ROOT / 'build.sh').read_text()
        ps = (ROOT / 'build.ps1').read_text()
        self.assertIn('npm --prefix desktop run build', shell)
        self.assertIn('npm --prefix desktop run build', ps)
        self.assertNotIn('rebuild: delete it first', shell)
        self.assertNotIn('rebuild: delete it first', ps)

    def test_windows_embeds_library_and_uses_app_fullscreen(self):
        main = (ROOT / 'src-tauri/src/main.rs').read_text()
        native = (ROOT / 'src-tauri/src/native.rs').read_text()
        renderer = (ROOT / 'src-tauri/src/embedded_windows.rs').read_text()
        frontend = (ROOT / 'desktop/src/App.tsx').read_text()
        self.assertIn('embedded_windows.rs', main)
        self.assertIn('libmpv-2.dll', renderer)
        self.assertIn('mpv_initialize', renderer)
        self.assertIn('"wid"', renderer)
        self.assertNotIn('Command::new', renderer)
        self.assertIn('player.start()', native)
        self.assertIn('window.set_fullscreen(enabled)', native)
        self.assertIn('/Linux|Windows/', frontend)
        self.assertNotIn('Windows currently uses a separate mpv window', frontend)

    def test_only_native_linux_and_windows_artifacts_are_published(self):
        shell = (ROOT / 'build.sh').read_text()
        ps = (ROOT / 'build.ps1').read_text()
        for script in [shell, ps]:
            self.assertNotRegex(script, r'\bgo\s+build\b')
            self.assertNotIn('-standalone-', script)
            self.assertNotIn('-server-', script)
            self.assertNotIn('-darwin-', script)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            shutil.copy2(ROOT / 'build.sh', root / 'build.sh')
            tools = root / 'tools'
            tools.mkdir()
            # Explicit build-tool fixtures: exercise the real script without cross-compiling in a unit test.
            fixtures = {
                'npm': '#!/bin/sh\nexit 0\n',
                'uname': '#!/bin/sh\nprintf "Linux\\n"\n',
                'cargo-xwin': '#!/bin/sh\nexit 0\n',
                'cargo': '''#!/usr/bin/env python3
import os, sys
from pathlib import Path
target = sys.argv[sys.argv.index('--target') + 1]
name = 'dasiwa-simple-video-compare' + ('.exe' if 'windows' in target else '')
out = Path(os.environ['CARGO_TARGET_DIR']) / target / 'release' / name
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text('explicit desktop build fixture: ' + target)
out.chmod(0o755)
''',
            }
            for name, contents in fixtures.items():
                executable = tools / name
                executable.write_text(contents)
                executable.chmod(0o755)
            env = {**os.environ, 'PATH': str(tools) + os.pathsep + os.environ['PATH'], 'CARGO_TARGET_DIR': str(root / '.target')}
            result = subprocess.run(['bash', 'build.sh'], cwd=root, env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual({p.name for p in root.glob('dasiwa-simple-video-compare-*')}, {
                'dasiwa-simple-video-compare-linux-amd64',
                'dasiwa-simple-video-compare-windows-amd64.exe',
            })
            rejected = subprocess.run(['bash', 'build.sh', '--webui-only'], cwd=root, env=env, capture_output=True, text=True)
            self.assertEqual(rejected.returncode, 2)

if __name__ == '__main__':
    unittest.main()
