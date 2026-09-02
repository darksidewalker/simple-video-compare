# DaSiWa Simple Video Compare

![Preview](assets/preview.png)

A tool for comparing two local video files side by side in a browser window. Files play through the browser's `<video>` element; for codecs a browser cannot decode (HEVC, AV1, VP9), the local server transcodes a compatibility proxy with ffmpeg.

## Features

- Standalone UX: single binary, the web interface opens in the default browser (opt-out with `--no-browser`)
- Browser playback with an ffmpeg-based H.264 compatibility proxy, so HEVC/AV1/VP9 sources play in browsers that cannot decode them natively
- Self-contained ffmpeg: if ffmpeg/ffprobe are missing, the app can download a pinned standalone build into the app folder (or the user installs them manually)
- Local file selection: built-in file browser, file drop directly onto the left/right video pane, or paste a path; two files dropped at once fill A and B
- Web UI: the web interface runs as an embedded local HTTP server (port 1420, `DAWVC_PORT` to override)
- Browser auto-start: when started in a graphical environment, the web interface opens automatically in the default browser; `--no-browser` disables this
- Platforms: Windows, Linux and macOS (amd64) — pure Go binaries with no external runtime dependencies (only ffmpeg/ffprobe)

## Requirements

- ffmpeg and ffprobe at runtime (details panel, codec probe, and the H.264 compatibility proxy for browser playback of HEVC/AV1/VP9 sources) — see below if missing

**No build is needed.** Ready-to-run platform binaries are committed to the repository root; just run the one for your platform.

| File | Platform | Launch |
|---|---|---|
| `dasiwa-simple-video-compare-linux-amd64` | Linux | `./dasiwa-simple-video-compare-linux-amd64` |
| `dasiwa-simple-video-compare-darwin-amd64` | macOS (Intel) | `./dasiwa-simple-video-compare-darwin-amd64` |
| `dasiwa-simple-video-compare-windows-amd64.exe` | Windows | `dasiwa-simple-video-compare-windows-amd64.exe` |

If ffmpeg/ffprobe are not found, the app does **not** fail. It detects them in this order on every check:

1. `DAWVC_FFMPEG` / `DAWVC_FFPROBE` environment variable (absolute path, or a name resolved via `PATH`/`PATHEXT`)
2. The app folder (next to the executable) — where the built-in installer downloads standalone builds
3. The system `PATH`

Open the **Runtime** dialog in the web UI when the tools are missing. It offers two choices:

- **Download standalone build** — fetches a pinned static build (BtbN/FFmpeg-Builds) and places `ffmpeg`/`ffprobe` next to the executable. Available on Linux and Windows; on macOS there is no pinned official build, so use the manual option.
- **Re-check** / install on your own — install via your package manager (or `brew` on macOS), or drop `ffmpeg`/`ffprobe` into the app folder (shown in the dialog), then re-check. No restart needed.

## Quick start

Start the binary for your platform from the repository root (table above). The web interface opens automatically in the default browser:

```bash
./dasiwa-simple-video-compare-linux-amd64        # Linux
./dasiwa-simple-video-compare-darwin-amd64       # macOS (Intel)
dasiwa-simple-video-compare-windows-amd64.exe    # Windows
```

Useful options:

```bash
--no-browser              # do not open the browser on start (daemon use)
DAWVC_PORT=8080 ...       # override the port (default 1420)
DAWVC_FFMPEG=/path/...    # point at a custom ffmpeg/ffprobe (see above)
```

## Building from source (developers only)

Only needed if you want to recompile the binaries yourself — normal use does not require Go.

```bash
./build.sh    # Linux, macOS and Windows in one pass
```

The binaries are pure Go (CGO off) and embed the web interface; at runtime they only need `ffmpeg`/`ffprobe`.

## Project structure

```
├── cmd/dasiwa-simple-video-compare/
│   ├── main.go              # Entry point for the desktop UX
│   └── web/                 # Embedded assets for the optional browser mode
├── internal/
│   ├── media/               # ffmpeg/ffprobe resolution, standalone installer
│   └── server/              # Embedded local HTTP server
├── assets/                  # Project assets (screenshots, icons)
└── go.mod                   # Go module definition
```

## License

Proprietary development as part of DaSiWa Tooling.
