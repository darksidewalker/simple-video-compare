# DaSiWa Simple Video Compare Implementation Plan

> For Hermes: implement phase-by-phase only after user explicitly approves the next phase. Use clean-code and go-embedded-web-app skills. Keep files small and responsibilities separated.

Goal: Build DaSiWa Simple Video Compare as a small Go-based standalone UX tool with dark-cyber web UI for side-by-side, slider overlay, blend, and difference video comparison.

Architecture: A single Go binary starts a local HTTP server and opens a standalone browser/app window when possible. The UI uses embedded HTML/CSS/JS with no frontend framework. FFmpeg/ffprobe are resolved from PATH first and used for metadata, compatibility proxies, thumbnails, and optional difference/preview generation.

Tech Stack: Go stdlib net/http, go:embed, HTML/CSS/vanilla JS, FFmpeg/ffprobe from PATH, browser media APIs. Linux first, Windows planned.

---

## Confirmed User Choices

- Project location: current repo `/home/darksidewalker/GitHub/DaSiWa-simple-video-compare`
- Tool name: `DaSiWa Simple Video Compare`
- Target: Linux first, Windows planned
- FFmpeg source: PATH for now
- Input UX: drag-and-drop field, plus manual path entry fallback
- Compare modes: side-by-side, slider overlay, blend/onion, and difference mode
- Sync: both hard-sync and offset/manual sync modes
- Standalone UX: preferred; use browser/webview strategy only if practical without heavy dependencies
- Cache: cache generated proxies/previews
- Visual style: DaSiWa dark-cyber style

---

## Key Decision: Standalone UX Without Bloat

Preferred implementation path:

1. Phase 1 uses local web UI opened in the default browser because it keeps the binary tiny and portable.
2. The code is structured so a later standalone shell can be added without rewriting the app.
3. If true standalone window is required in Phase 2/3, evaluate two options:
   - Linux: `xdg-open --app` style is browser-dependent and not reliable.
   - Webview/Wails/Tauri/Fyne add size/dependencies.
4. Recommended compromise:
   - Start with embedded web app as the core.
   - Add optional `--standalone` launcher mode later that tries installed Chromium/Chrome app-window mode first.
   - Keep native-webview as optional build tag later, not default.

Reason: user requested “as small as possible”; Electron/Tauri would violate that.

---

## Repository Baseline

Current repo appears empty from file search. Plan assumes greenfield implementation.

---

## Proposed Project Structure

Create:

```text
/home/darksidewalker/GitHub/DaSiWa-simple-video-compare/
  go.mod
  README.md
  .gitignore
  cmd/dasiwa-simple-video-compare/
    main.go
    web/
      index.html
      css/app.css
      js/api.js
      js/app.js
      js/compare.js
      js/dropzone.js
  internal/app/
    config.go
    browser.go
    paths.go
  internal/ffmpeg/
    binary.go
    probe.go
    proxy.go
    diff.go
  internal/media/
    video.go
    validate.go
    cache.go
  internal/server/
    server.go
    routes.go
    handlers_health.go
    handlers_media.go
    handlers_cache.go
    json.go
    static.go
  internal/compare/
    mode.go
    sync.go
  build/
    build.sh
```

---

## Public CLI Shape

Binary name:

```bash
dasiwa-simple-video-compare
```

Flags:

```bash
--host 127.0.0.1
--port 8765
--no-open
--standalone
--cache-dir ~/.cache/dasiwa-simple-video-compare
--ffmpeg ffmpeg
--ffprobe ffprobe
```

Defaults:

- Host: `127.0.0.1`
- Port: `8765`
- FFmpeg/ffprobe: resolved from PATH
- Cache dir: `$XDG_CACHE_HOME/dasiwa-simple-video-compare` or `~/.cache/dasiwa-simple-video-compare`
- Browser opens automatically unless `--no-open`

---

## API Design

### GET /health

Returns:

```json
{
  "status": "ok",
  "app": "DaSiWa Simple Video Compare",
  "version": "dev"
}
```

### GET /api/runtime

Returns ffmpeg availability and runtime paths:

```json
{
  "ffmpeg": "/usr/bin/ffmpeg",
  "ffprobe": "/usr/bin/ffprobe",
  "cache_dir": "/home/user/.cache/dasiwa-simple-video-compare",
  "platform": "linux"
}
```

### POST /api/probe

Request:

```json
{
  "path": "/absolute/path/video.mkv"
}
```

Returns:

```json
{
  "path": "/absolute/path/video.mkv",
  "duration": 12.34,
  "width": 1920,
  "height": 1080,
  "fps": 24.0,
  "video_codec": "h264",
  "audio_codec": "aac",
  "pixel_format": "yuv420p",
  "browser_likely_supported": true
}
```

### POST /api/proxy

Creates browser-compatible cached proxy.

Request:

```json
{
  "path": "/absolute/path/video.mkv",
  "force": false
}
```

Returns job id:

```json
{
  "job_id": "abc123"
}
```

### GET /api/jobs/{id}/events

SSE stream:

```json
{"type":"progress","percent":42,"message":"Encoding preview proxy"}
{"type":"done","url":"/cache/proxies/abc123.mp4"}
```

### GET /cache/{token}/{filename}

Serves cached generated media through a token registry, not raw filesystem paths.

---

## Compare Modes

### 1. Side-by-side

- Two HTML5 video elements in a CSS grid
- Shared transport controls
- Optional “fit contain/cover/original”
- Audio selector: A, B, mute

### 2. Slider overlay

- Videos stacked absolutely
- Top video clipped with CSS `clip-path` or width mask
- Range slider controls split position
- Sync transport shared

### 3. Blend/onion

- Videos stacked
- Opacity slider controls Video B over Video A
- Useful for alignment and subtle changes

### 4. Difference

Two levels:

- Phase 3 browser-live difference using Canvas if same resolution and browser allows frame drawing.
- Phase 4 FFmpeg-generated difference preview for robust codec/resolution handling.

Canvas difference risks:

- Cross-origin not an issue because files are served locally from same server.
- Performance may be limited for 4K.
- Frame-accurate difference is not guaranteed with regular HTML5 playback.

FFmpeg difference approach:

```bash
ffmpeg -i A -i B -filter_complex "[0:v][1:v]blend=all_mode=difference" -c:v libx264 -crf 18 -preset veryfast output.mp4
```

---

## Sync Modes

### Hard sync

- One primary clock, likely Video A
- On play/pause/seek, apply same state to B
- Periodically check drift
- If drift exceeds threshold, seek B to A + offset
- Default threshold: 80 ms

### Manual/offset sync

- User can set offset in ms
- Buttons:
  - B -1 frame
  - B +1 frame
  - B -100ms
  - B +100ms
  - reset offset
- UI displays active offset

---

## Cache Design

Cache key:

- absolute path
- file size
- modification time
- operation type: proxy/diff/thumb
- settings hash

Cache paths:

```text
~/.cache/dasiwa-simple-video-compare/
  manifest.json
  proxies/
  difference/
  thumbs/
```

Manifest entry:

```json
{
  "key": "sha256...",
  "source_path": "/path/video.mkv",
  "source_size": 123456,
  "source_mtime": "2026-07-08T15:00:00Z",
  "kind": "proxy",
  "output": "proxies/sha256.mp4",
  "created": "2026-07-08T15:00:00Z"
}
```

---

## Phase 1: Minimal Working App

Objective: Build the smallest useful version: local server, embedded UI, drag/drop/manual paths, side-by-side playback, basic sync.

Files:

- Create `go.mod`
- Create `cmd/dasiwa-simple-video-compare/main.go`
- Create `cmd/dasiwa-simple-video-compare/web/index.html`
- Create `cmd/dasiwa-simple-video-compare/web/css/app.css`
- Create `cmd/dasiwa-simple-video-compare/web/js/api.js`
- Create `cmd/dasiwa-simple-video-compare/web/js/app.js`
- Create `cmd/dasiwa-simple-video-compare/web/js/compare.js`
- Create `cmd/dasiwa-simple-video-compare/web/js/dropzone.js`
- Create `internal/server/server.go`
- Create `internal/server/routes.go`
- Create `internal/server/json.go`
- Create `internal/server/handlers_health.go`
- Create `internal/app/browser.go`

Implementation steps:

1. Initialize Go module.
2. Add embedded web assets with `//go:embed web`.
3. Add HTTP server with `/health`, `/api/runtime`, static UI.
4. Add browser auto-open via `xdg-open` on Linux.
5. Build DaSiWa dark-cyber shell UI.
6. Add two drop zones and manual path inputs.
7. For Phase 1, accept drag/drop only if browser exposes file object URL for local preview; manual path will require server serving in Phase 2.
8. Add side-by-side video elements.
9. Add shared play/pause/seek controls.
10. Add basic sync from primary video A to video B.
11. Add mode selector with only side-by-side enabled; other modes visible but marked “Phase 2”.

Verification:

```bash
go test ./...
go vet ./...
go build -o ./dist/dasiwa-simple-video-compare ./cmd/dasiwa-simple-video-compare
./dist/dasiwa-simple-video-compare --no-open
curl http://127.0.0.1:8765/health
```

Manual browser verification:

- Load two browser-compatible MP4 files by drag/drop.
- Press play: both videos play.
- Pause: both pause.
- Seek: both seek.
- UI fits dark-cyber style.

Acceptance criteria:

- Binary starts on Linux.
- UI loads.
- No external dependencies except browser.
- Two videos can be compared side-by-side if browser-compatible.
- Shared playback controls work.

---

## Phase 2: Local Path Serving + ffprobe Metadata

Objective: Make manual paths work reliably and show technical metadata.

Files:

- Create `internal/ffmpeg/binary.go`
- Create `internal/ffmpeg/probe.go`
- Create `internal/media/video.go`
- Create `internal/media/validate.go`
- Create `internal/server/handlers_media.go`
- Modify `internal/server/routes.go`
- Modify frontend API/UI files

Implementation steps:

1. Add ffmpeg/ffprobe resolver from PATH.
2. Add `POST /api/probe`.
3. Parse ffprobe JSON into clean Go structs.
4. Add local media token registry so `/media/{token}` maps safely to selected absolute paths.
5. Add `POST /api/media/register` returning a local URL for browser playback.
6. Update manual path input to call register + probe.
7. Show metadata cards for A and B.
8. Add compatibility warning if codec/container likely unsupported by browser.

Verification:

```bash
go test ./...
go vet ./...
go build -o ./dist/dasiwa-simple-video-compare ./cmd/dasiwa-simple-video-compare
ffprobe -version
curl -X POST http://127.0.0.1:8765/api/probe -d '{"path":"/path/to/test.mp4"}' -H 'Content-Type: application/json'
```

Acceptance criteria:

- Manual absolute path works.
- Metadata displays.
- Media is served safely by token, not direct path.
- Browser can play registered compatible files.

---

## Phase 3: All Live Compare Modes

Objective: Implement side-by-side, slider overlay, blend/onion, and browser-live difference where possible.

Files:

- Create `internal/compare/mode.go`
- Create `internal/compare/sync.go`
- Modify `cmd/.../web/js/compare.js`
- Modify `cmd/.../web/css/app.css`
- Modify `cmd/.../web/index.html`

Implementation steps:

1. Refactor frontend compare state into `compare.js`.
2. Add mode switcher:
   - side-by-side
   - slider
   - blend
   - difference
3. Implement slider overlay with CSS clipping.
4. Implement blend mode with opacity slider.
5. Implement offset controls.
6. Implement hard sync toggle.
7. Add drift monitor.
8. Implement canvas live difference only when both videos are loaded and dimensions are valid.
9. Show fallback message for live difference if performance/codec prevents it.

Verification:

- Side-by-side still works.
- Slider moves smoothly.
- Blend opacity works.
- Offset changes B relative to A.
- Hard sync can be disabled.
- Difference mode either works or clearly reports fallback needed.

Acceptance criteria:

- All requested modes are visible and functional or have clear FFmpeg fallback path.
- No duplicated player code.
- Compare state is centralized.

---

## Phase 4: FFmpeg Proxy, Difference Preview, and Cache

Objective: Add reliable compatibility and cached generated media.

Files:

- Create `internal/media/cache.go`
- Create `internal/ffmpeg/proxy.go`
- Create `internal/ffmpeg/diff.go`
- Create `internal/server/handlers_cache.go`
- Modify `internal/server/routes.go`
- Modify frontend API/UI files

Implementation steps:

1. Implement cache directory resolver.
2. Implement cache manifest load/save.
3. Implement cache key creation.
4. Implement proxy encode:
   - H.264 MP4
   - AAC audio if needed
   - `-movflags +faststart`
5. Add `POST /api/proxy`.
6. Add SSE job stream for progress.
7. Add cached media serving by token.
8. Add UI button “Create browser preview”.
9. Auto-suggest preview if browser likely cannot play source.
10. Implement FFmpeg difference preview generation.
11. Reuse cached outputs if source and settings match.

Verification:

```bash
go test ./...
go vet ./...
go build -o ./dist/dasiwa-simple-video-compare ./cmd/dasiwa-simple-video-compare
```

Manual:

- Load MKV/unsupported codec.
- Click Create browser preview.
- See progress.
- Generated proxy plays.
- Restart app and same proxy is reused from cache.
- Difference preview generation works for two test files.

Acceptance criteria:

- FFmpeg from PATH is used.
- Generated previews are cached.
- Cache invalidates when source file changes.
- SSE progress works without false disconnect messages.

---

## Phase 5: Standalone UX Launcher and Windows Preparation

Objective: Improve standalone feel while keeping app small.

Files:

- Modify `internal/app/browser.go`
- Modify `cmd/.../main.go`
- Create/update `build/build.sh`
- Update `README.md`

Implementation steps:

1. Add `--standalone` flag.
2. Linux standalone launcher attempts:
   - chromium/chrome with `--app=http://127.0.0.1:8765`
   - fallback to `xdg-open`
3. Add single-instance port probe.
4. If already running, open existing app and exit 0.
5. Add graceful shutdown on SIGINT/SIGTERM.
6. Add Linux build script with stripped binary.
7. Add Windows build target placeholder.
8. Document Windows FFmpeg PATH requirement.

Verification:

```bash
./build/build.sh
dist/dasiwa-simple-video-compare-linux-amd64 --standalone
```

Acceptance criteria:

- Linux standalone mode opens a clean app-like window when Chromium/Chrome exists.
- Fallback browser still works.
- Duplicate launch opens existing app instead of failing.
- Build artifact is small.

---

## Phase 6: Polish and Documentation

Objective: Make the tool pleasant and maintainable.

Files:

- Update `README.md`
- Update frontend CSS/JS
- Add screenshots later if requested

Implementation steps:

1. Add README with install/build/run instructions.
2. Add troubleshooting:
   - FFmpeg not found
   - browser cannot play codec
   - proxy cache location
   - Linux standalone launcher fallback
3. Add keyboard shortcuts:
   - Space play/pause
   - Left/right seek
   - Shift-left/right offset
   - 1/2/3/4 mode switching
4. Add cache clear button.
5. Add “copy debug info” button.

Verification:

- README commands work.
- Keyboard shortcuts work.
- Cache clear removes generated files safely.

---

## Testing Strategy

Go unit tests:

- ffmpeg binary resolution
- ffprobe JSON parsing using fixture JSON
- video path validation
- cache key stability
- cache invalidation on size/mtime change
- media token registry path traversal prevention

Manual integration tests:

- Browser-compatible MP4 pair
- MKV unsupported pair with proxy generation
- Different resolution pair
- Different duration pair
- Offset sync
- Cache reuse after restart

Commands:

```bash
go test ./...
go vet ./...
go build -o ./dist/dasiwa-simple-video-compare ./cmd/dasiwa-simple-video-compare
```

---

## Risks and Mitigations

Risk: True standalone GUI conflicts with “as small as possible”.
Mitigation: Browser-app-window mode first; native webview only optional later.

Risk: Browser drag/drop cannot provide full local path.
Mitigation: Drop supports direct object URL preview; manual absolute path supports backend ffprobe/register. If local full-path drag/drop is required, native shell/webview would be needed.

Risk: HTML5 video sync is not frame-perfect.
Mitigation: Hard sync with drift correction + manual offset controls. Document limitation.

Risk: Difference mode is expensive live.
Mitigation: Provide live canvas difference only when practical; FFmpeg cached difference preview for robust mode.

Risk: Browser codec support varies.
Mitigation: ffprobe + proxy generation.

Risk: Serving arbitrary local files can be dangerous.
Mitigation: Token registry, no raw path in URL, path traversal checks, localhost-only bind by default.

---

## Immediate Next Step After Approval

Start Phase 1 only.

Phase 1 deliverable:

- Compiling Go binary
- Embedded dark-cyber UI
- Drag/drop two browser-compatible videos
- Side-by-side playback
- Shared controls
- Basic sync
- Verified with `go test`, `go vet`, `go build`, and `/health` curl

Do not implement FFmpeg/proxy/cache until Phase 2/4 unless Phase 1 reveals a blocker.
