import importlib.util
from pathlib import Path
from unittest import TestCase, main
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('package_linux', Path(__file__).resolve().parents[1] / 'scripts/package_linux.py')
assert spec is not None and spec.loader is not None
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class LinuxRuntimePackageTests(TestCase):
    def test_leaves_platform_libraries_on_host_but_bundles_codecs(self):
        for name in ['libc.so.6', 'libGL.so.1', 'libOpenCL.so.1', 'libwayland-client.so.0', 'libgio-2.0.so.0']:
            self.assertIsNotNone(package.SYSTEM.match(name), name)
        for name in ['libmpv.so.2', 'libavcodec.so.63', 'libdav1d.so.7', 'libcdio.so.21', 'libmp3lame.so.0']:
            self.assertIsNone(package.SYSTEM.match(name), name)

    @patch.object(package.subprocess, 'run')
    def test_ldd_handles_named_and_absolute_dependencies_and_rejects_missing(self, run):
        run.return_value.stdout = 'libcodec.so.1 => /usr/lib/libcodec.so.1 (0x1)\n/usr/lib/libmujs.so (0x2)\nlinux-vdso.so.1 (0x3)\n'
        self.assertEqual(package.dependencies('/library'), [Path('/usr/lib/libcodec.so.1'), Path('/usr/lib/libmujs.so')])
        run.return_value.stdout = 'libcodec.so.1 => not found\n'
        with self.assertRaises(RuntimeError):
            package.dependencies('/library')


if __name__ == '__main__':
    main()
