#!/usr/bin/env python3
"""Package the current Linux build with libmpv, ffprobe and their shared libraries.
This is a build-host runtime bundle, not a universal Linux/AppImage release.
"""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
# Keep glibc and the graphics/display stack on the host; never ship GPU drivers.
SYSTEM = re.compile(r'^(?:ld-linux|lib(?:c\.|m\.|mvec\.|dl\.|pthread\.|rt\.|resolv\.|nss_|GL|EGL|OpenGL|OpenCL|vulkan|va(?:-|\.)|vdpau|drm|gbm|wayland|X|xcb|xkbcommon|gtk|gdk|glib|gobject|gio|gmodule|gthread|cairo|pango|fontconfig|freetype|harfbuzz|pixman|gdk_pixbuf))')


def dependencies(path):
    result = subprocess.run(['ldd', str(path)], check=True, capture_output=True, text=True)
    if 'not found' in result.stdout:
        raise RuntimeError(f'Unresolved dependency for {path}:\n{result.stdout}')
    paths = []
    for line in result.stdout.splitlines():
        match = re.search(r'(?:=>\s+)?(/\S+)\s+\(', line)
        if match:
            paths.append(Path(match.group(1)))
    return paths


def main():
    assert sys.platform == 'linux', 'This packager is Linux-only'
    binary = ROOT / 'dasiwa-simple-video-compare-linux-amd64'
    assert binary.is_file(), 'Build the native Linux desktop first'
    ffprobe = Path(shutil.which('ffprobe') or '')
    assert ffprobe.is_file(), 'ffprobe is required on the packaging host'
    cache = subprocess.run(['ldconfig', '-p'], check=True, capture_output=True, text=True).stdout
    match = re.search(r'libmpv\.so\.2\s+.*=>\s+(\S+)', cache)
    assert match, 'libmpv.so.2 is required on the packaging host'
    libmpv = Path(match.group(1))
    output = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / 'dist/dasiwa-simple-video-compare-native-linux-x86_64.tar.gz'
    output = output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='vc-bundle-', dir=os.environ.get('TMPDIR')) as temporary:
        package = Path(temporary) / 'DaSiWa-simple-video-compare'
        libs = package / 'lib'
        libs.mkdir(parents=True)
        shutil.copy2(binary, package / 'dasiwa-simple-video-compare')
        shutil.copy2(ffprobe, package / 'ffprobe')
        shutil.copy2(ROOT / 'LICENSE', package / 'LICENSE')
        included = {}
        system = set()
        for source in [libmpv, *dependencies(libmpv), *dependencies(ffprobe)]:
            if SYSTEM.match(source.name):
                system.add(source.name)
                continue
            target = libs / source.name
            if target.name in included and Path(included[target.name]).resolve() != source.resolve():
                raise RuntimeError(f'Conflicting shared library: {target.name}')
            shutil.copy2(source, target, follow_symlinks=True)
            included[target.name] = str(source)
        (package / 'runtime-manifest.json').write_text(json.dumps({'build_host': os.uname().sysname, 'included': included, 'host_libraries': sorted(system)}, indent=2) + '\n')
        launcher = package / 'start.sh'
        launcher.write_text('''#!/bin/sh
set -eu
base=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
export LD_LIBRARY_PATH="$base/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export DAWVC_LIBMPV="$base/lib/libmpv.so.2"
export DAWVC_FFPROBE="$base/ffprobe"
exec "$base/dasiwa-simple-video-compare" "$@"
''')
        launcher.chmod(0o755)
        (package / 'RUNTIME-NOTICE.txt').write_text('''Local build-host runtime bundle. Run ./start.sh after extracting.
Includes libmpv, ffprobe and codec/support libraries from the packaging host.
Does not install or change system files. GTK/WebKitGTK, glibc and GPU drivers
remain host dependencies. This bundle is not certified for older Linux releases.

This is a local development artifact, not a public redistributable release.
Before public distribution, provide the corresponding source and notices for
all bundled packages. runtime-manifest.json records exact source file paths;
the project LICENSE alone is not a substitute for third-party source/notices.
''')
        probe = subprocess.run([str(package / 'ffprobe'), '-version'], env={**os.environ, 'LD_LIBRARY_PATH': str(libs)}, check=True, capture_output=True, text=True)
        assert 'ffprobe version' in probe.stdout
        with tarfile.open(output, 'w:gz') as archive:
            archive.add(package, arcname=package.name)
        print(f'Created {output}; bundled {len(included)} shared libraries; packaged ffprobe executes.')


if __name__ == '__main__':
    main()
