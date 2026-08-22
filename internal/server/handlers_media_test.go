package server

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"

	"dasiwa-simple-video-compare/internal/media"
)

// writeFakeFFprobe writes an executable ffprobe stand-in that answers the
// server's JSON probe invocation. streams are the JSON stream objects to emit.
func writeFakeFFprobe(t *testing.T, dir, codecName string) {
	t.Helper()
	script := "#!/bin/sh\n" +
		"cat <<'JSON'\n" +
		`{"streams":[{"index":0,"codec_type":"video","codec_name":"` + codecName + `"}]}` + "\n" +
		"JSON\n"
	if err := os.WriteFile(filepath.Join(dir, "ffprobe"), []byte(script), 0755); err != nil {
		t.Fatal(err)
	}
}

// writeFakeFFmpeg writes an executable ffmpeg stand-in that succeeds and
// creates the output file (the last argument) so ensureCompatibleMedia passes.
func writeFakeFFmpeg(t *testing.T, dir string) {
	t.Helper()
	script := "#!/bin/sh\nlast=\"\"\nfor a in \"$@\"; do last=\"$a\"; done\n" +
		"echo transcoded > \"$last\"\n"
	if err := os.WriteFile(filepath.Join(dir, "ffmpeg"), []byte(script), 0755); err != nil {
		t.Fatal(err)
	}
}

// Regression: Supernova.mp4-style files make ffprobe's flat format emit the
// codec line twice ("hevc\nhevc"), which defeated the old exact-string match
// and left the raw HEVC stream in the browser video element (black frame).
// The JSON-based probe must classify HEVC as incompatible and register the
// H.264 proxy.
func TestRegisterMediaTranscodesHevcWhenCodecProbeWorks(t *testing.T) {
	dir := t.TempDir()
	videoPath := filepath.Join(dir, "clip.mp4")
	mustWrite(t, videoPath, "video")
	writeFakeFFprobe(t, dir, "hevc")
	writeFakeFFmpeg(t, dir)

	// Keep proxy output inside the test's temp dir, away from the shared
	// /tmp/dasiwa-simple-video-compare cache a real run uses.
	t.Setenv("TMPDIR", t.TempDir())

	tools := media.Tools{
		FFmpeg:  media.Tool{Name: "ffmpeg", Path: filepath.Join(dir, "ffmpeg"), Available: true},
		FFprobe: media.Tool{Name: "ffprobe", Path: filepath.Join(dir, "ffprobe"), Available: true},
		Ready:   true,
	}
	handler := NewWithConfig(testAssets(), "test", Config{RootDir: dir, Tools: tools})

	data, err := json.Marshal(map[string]string{"path": videoPath})
	if err != nil {
		t.Fatal(err)
	}
	req := httptest.NewRequest(http.MethodPost, "/api/media/register", bytes.NewBuffer(data))
	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("register status = %d, body = %s", rec.Code, rec.Body.String())
	}
	var entry MediaEntry
	if err := json.Unmarshal(rec.Body.Bytes(), &entry); err != nil {
		t.Fatal(err)
	}
	if entry.Compatible {
		t.Fatalf("expected HEVC file to need the H.264 proxy, entry = %+v", entry)
	}
	if entry.Name == "" || filepath.Ext(entry.Name) != ".mp4" || filepath.Base(entry.Path) == "clip.mp4" {
		t.Fatalf("expected proxy path for HEVC source, entry = %+v", entry)
	}
}

func TestProbeFirstVideoCodecIgnoresNonVideoStreams(t *testing.T) {
	dir := t.TempDir()
	videoPath := filepath.Join(dir, "clip.mp4")
	mustWrite(t, videoPath, "video")
	writeFakeFFmpeg(t, dir)

	tools := media.Tools{
		FFmpeg:  media.Tool{Name: "ffmpeg", Path: filepath.Join(dir, "ffmpeg"), Available: true},
		FFprobe: media.Tool{Name: "ffprobe", Path: filepath.Join(dir, "ffprobe"), Available: true},
		Ready:   true,
	}
	srv := &Server{
		assets:  testAssets(),
		version: "test",
		config:  Config{RootDir: dir, Tools: tools},
		media:   make(map[string]MediaEntry),
		cache:   make(map[string][]byte),
		compat:  make(map[string]string),
	}

	// FFprobe script emitting only an audio stream -> no codec, no proxy.
	script := "#!/bin/sh\ncat <<'JSON'\n{\"streams\":[{\"index\":0,\"codec_type\":\"audio\",\"codec_name\":\"aac\"}]}\nJSON\n"
	if err := os.WriteFile(filepath.Join(dir, "ffprobe"), []byte(script), 0755); err != nil {
		t.Fatal(err)
	}
	if codec := srv.probeFirstVideoCodec(videoPath); codec != "" {
		t.Fatalf("expected empty codec for audio-only file, got %q", codec)
	}

	if srv.needsCompatibleProxy(videoPath) {
		t.Fatal("aac-only file must not need the H.264 proxy")
	}

	// AV1 must still trigger the proxy.
	writeFakeFFprobe(t, dir, "av1")
	if !srv.needsCompatibleProxy(videoPath) {
		t.Fatal("AV1 file must need the H.264 proxy")
	}

	// H.264 must not trigger the proxy.
	writeFakeFFprobe(t, dir, "h264")
	if srv.needsCompatibleProxy(videoPath) {
		t.Fatal("H.264 file must not need the H.264 proxy")
	}
}
