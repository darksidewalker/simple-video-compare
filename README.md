# DaSiWa Simple Video Compare

![Native Linux video comparison](assets/preview.png)

Compare two local videos with a Rust + React desktop app. Linux embeds libmpv in a native GTK/OpenGL video pane; the Windows build currently opens a separate mpv video window. The React interface controls playback without decoding videos in the browser. Original source files are never rewritten.

The preview shows the Linux app with generated demonstration videos.

## Download and start

There are **two desktop binaries**, both for amd64. No separate server, browser-app or `standalone` variant is distributed. macOS is not a release target.

| Platform | Binary | Video runtime |
|---|---|---|
| Linux | [dasiwa-simple-video-compare-linux-amd64](dasiwa-simple-video-compare-linux-amd64) | `libmpv.so.2`, `ffprobe`, GTK3 and WebKitGTK 4.1 |
| Windows | [dasiwa-simple-video-compare-windows-amd64.exe](dasiwa-simple-video-compare-windows-amd64.exe) | `mpv.exe`, `ffprobe.exe` and Microsoft Edge WebView2 Runtime |

On Linux:

```fish
chmod +x ./dasiwa-simple-video-compare-linux-amd64
./dasiwa-simple-video-compare-linux-amd64
```

On Windows, launch `dasiwa-simple-video-compare-windows-amd64.exe`. Put mpv and ffprobe beside the executable or add them to PATH. The desktop app does not download these tools automatically.

Linux searches for `lib/libmpv.so.2` beside the app, then `libmpv.so.2` beside the app, then the system library. `DAWVC_LIBMPV` overrides the Linux library path. `DAWVC_FFPROBE` overrides ffprobe; `DAWVC_MPV` selects the external Windows player. No Go sidecar, localhost server or compatibility transcode is needed by the desktop app.

The Linux executable has been exercised with real native rendering. The Windows executable is cross-compiled from the same desktop source, but playback has not been verified on Windows.

## Comparing videos

Browse, drop a file onto source A or B, or paste an absolute path. Two selected or dropped files fill A and B in order. A single populated slot also works for ordinary playback. The timeline uses the longer source's duration and holds the shorter source's last frame at its end.

- **Side:** both aligned sources side by side.
- **Slider:** wipe between A and B. On Linux, drag the line or its handle directly over the video, use the A/B split control below, or focus the divider and use the left/right arrow keys.
- **Blend:** mix the two aligned frames.
- **Diff:** show the aligned frame difference.
- **Playback:** play/pause, seek, frame stepping, synchronization, fullscreen, audio A/B/mute and volume.
- **Source details:** original ffprobe metadata and raw JSON, not proxy metadata.

The Linux slider caches the packed A/B frame on the GPU. Dragging changes the displayed regions without rebuilding the video filter, seeking or decoding the paused frame again. It also works while playback continues. Blend and the external Windows player retain the filter-based comparison path.

When the control desk has focus, Space toggles playback, the left/right arrows seek five seconds, and Shift + arrow steps one frame. Text fields, selectors and buttons keep their normal keyboard behavior.

Comparisons scale/pad B to A's geometry and currently use 8-bit YUV 4:2:0. This is not a bit-exact HDR/high-bit-depth analysis tool. Linux currently uses software decoding with GPU rendering; slider compositing does not remove the cost of decoding large or complex sources. Runtime codec support depends on the installed mpv/FFmpeg build.

## Build

Linux and Windows are the only distributed targets. Node/npm, Rust and the platform's Tauri build dependencies are required. Linux also needs the GTK3/WebKitGTK 4.1 development packages and libmpv for runtime testing.

On Linux:

```fish
./build.sh
```

This rebuilds React and the Linux desktop executable. With `cargo-xwin` and the `x86_64-pc-windows-msvc` Rust target installed, it also cross-compiles the Windows executable. If the cross-toolchain is missing, the script explicitly reports that Windows was skipped. `CARGO_TARGET_DIR` selects a reusable build directory; the default is `.target`.

On Windows, use PowerShell:

```powershell
.\build.ps1
```

This rebuilds React and the native Windows executable. Both scripts publish the same names from the download table; they do not build Go browser/server variants. A running Linux instance is not stopped when its executable is replaced; restart the app to load the new build.

The legacy Go browser implementation and the old root Rust prototype remain source-only. They are not the distributed desktop app. The desktop Rust manifest is `src-tauri/Cargo.toml`.

## Verification

```fish
npm --prefix desktop test
npm --prefix desktop run build
python -m unittest discover -s tests -p '*_test.py'
go test ./...
env CARGO_TARGET_DIR="$PWD/.target" cargo test --manifest-path src-tauri/Cargo.toml
```

The Go tests cover the retained legacy source. Optional Rust integration tests require mpv, ffmpeg, ffprobe and libmpv:

```fish
env DAWVC_TEST_MEDIA=/absolute/path/to/video.webm CARGO_TARGET_DIR="$PWD/.target" cargo test --manifest-path src-tauri/Cargo.toml -- --include-ignored
```

For actual-app checks, launch a dedicated Linux instance with the local inspector enabled, then run either test from another terminal. Use sources longer than four seconds. Each test closes its dedicated instance:

```fish
env DAWVC_RENDER_VERIFY=1 WEBKIT_INSPECTOR_HTTP_SERVER=127.0.0.1:9238 ./dasiwa-simple-video-compare-linux-amd64
# In another terminal:
uv run --no-project --with websocket-client python desktop/tests/native_smoke.py /absolute/path/to/A.mp4 /absolute/path/to/B.webm
# Or, with a new dedicated instance:
uv run --no-project --with websocket-client python desktop/tests/slider_performance.py /absolute/path/to/A.mp4 /absolute/path/to/B.webm
```

The performance test measures control-command round trips, not full mouse-to-monitor latency. Opt-in `GLAREA_SPLIT` logs expose composed center pixels, GPU cache reuse and copy timings; `GLAREA_FRAME` logs include framebuffer hashes and OpenGL errors. Native mouse/pixel tests and two generated 3840×2160 sources have been exercised on Linux, including RTX 5090 rendering. This does not establish performance for every 4K/8K codec or file.

`scripts/package_linux.py` can assemble a local runtime archive from the canonical Linux binary, libmpv, ffprobe and the build host's codec libraries. GTK/WebKitGTK, glibc and GPU drivers remain host requirements. Before redistributing that optional archive, supply the corresponding third-party sources and license notices; it is not a universal Linux release.

## Source layout

- `desktop/`: React controls, frontend model and UI tests.
- `src-tauri/`: native desktop lifecycle, mpv IPC, file selection and Linux GTK/OpenGL renderer.
- `src-tauri/vendor/wry/`: vendored wry with WebKitGTK/Wayland file-drop fixes; third-party licenses are included.
- `tests/`, `scripts/`: desktop build/package checks and optional Linux runtime packaging.
- `cmd/`, `internal/`: retained legacy Go browser app and its tests.
- `assets/`: preview and application artwork.

## License

GNU General Public License, version 3 or any later version. See [LICENSE](LICENSE). Runtime mpv/FFmpeg components and vendored libraries retain their own licenses.
