package server

import (
	"bytes"
	"encoding/json"
	"io/fs"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"testing/fstest"

	"dasiwa-simple-video-compare/internal/media"
)

func TestBrowseFilesReturnsLocalVideoFiles(t *testing.T) {
	dir := t.TempDir()
	mustWrite(t, filepath.Join(dir, "clip.mp4"), "video")
	mustWrite(t, filepath.Join(dir, "notes.txt"), "ignore")
	if err := os.Mkdir(filepath.Join(dir, "folder"), 0755); err != nil {
		t.Fatal(err)
	}

	handler := NewWithConfig(testAssets(), "test", Config{RootDir: dir, Tools: media.Tools{}})
	req := httptest.NewRequest(http.MethodGet, "/api/browse?path="+dir, nil)
	rec := httptest.NewRecorder()

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, body = %s", rec.Code, rec.Body.String())
	}
	var got struct {
		Path  string       `json:"path"`
		Items []BrowseItem `json:"items"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &got); err != nil {
		t.Fatal(err)
	}
	if got.Path != dir {
		t.Fatalf("path = %q, want %q", got.Path, dir)
	}
	if len(got.Items) != 2 {
		t.Fatalf("items = %+v", got.Items)
	}
	if !got.Items[0].IsDir || got.Items[0].Name != "folder" {
		t.Fatalf("first item should be folder, got %+v", got.Items[0])
	}
	if got.Items[1].Name != "clip.mp4" {
		t.Fatalf("second item should be clip.mp4, got %+v", got.Items[1])
	}
}

func TestRegisterMediaReturnsPlayableLocalURL(t *testing.T) {
	dir := t.TempDir()
	videoPath := filepath.Join(dir, "clip.mkv")
	mustWrite(t, videoPath, "video")
	handler := NewWithConfig(testAssets(), "test", Config{RootDir: dir, Tools: media.Tools{}})
	data, err := json.Marshal(map[string]string{"path": videoPath})
	if err != nil {
		t.Fatal(err)
	}
	body := bytes.NewBuffer(data)
	req := httptest.NewRequest(http.MethodPost, "/api/media/register", body)
	rec := httptest.NewRecorder()

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, body = %s", rec.Code, rec.Body.String())
	}
	var got struct {
		URL string `json:"url"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &got); err != nil {
		t.Fatal(err)
	}
	if got.URL == "" {
		t.Fatal("empty media url")
	}
}

func TestRuntimeIncludesLocalUXAndFFmpegStatus(t *testing.T) {
	handler := NewWithConfig(testAssets(), "test", Config{
		RootDir: t.TempDir(),
		Tools: media.Tools{
			FFmpeg:  media.Tool{Name: "ffmpeg", Path: "/bin/ffmpeg", Available: true},
			FFprobe: media.Tool{Name: "ffprobe", Path: "/bin/ffprobe", Available: true},
			Ready:   true,
		},
	})
	req := httptest.NewRequest(http.MethodGet, "/api/runtime", nil)
	rec := httptest.NewRecorder()

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d", rec.Code)
	}
	var got RuntimeInfo
	if err := json.Unmarshal(rec.Body.Bytes(), &got); err != nil {
		t.Fatal(err)
	}
	if got.UXMode != "local-app-window" {
		t.Fatalf("ux mode = %q", got.UXMode)
	}
	if !got.Tools.Ready {
		t.Fatalf("expected ready tools, got %+v", got.Tools)
	}
}

func TestCacheMediaLoadsSmallVideoIntoRAMAndServesFromCache(t *testing.T) {
	dir := t.TempDir()
	videoPath := filepath.Join(dir, "clip.mp4")
	mustWrite(t, videoPath, "cached-video")
	handler := NewWithConfig(testAssets(), "test", Config{RootDir: dir, Tools: media.Tools{}, MaxRAMCacheBytes: 1024})
	entry := registerTestMedia(t, handler, videoPath)

	req := httptest.NewRequest(http.MethodPost, "/api/media/cache", strings.NewReader(`{"id":"`+entry.ID+`"}`))
	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("cache status = %d, body = %s", rec.Code, rec.Body.String())
	}
	var cached MediaEntry
	if err := json.Unmarshal(rec.Body.Bytes(), &cached); err != nil {
		t.Fatal(err)
	}
	if !cached.Cached || cached.CacheBytes != int64(len("cached-video")) {
		t.Fatalf("cache metadata = %+v", cached)
	}
	if err := os.Remove(videoPath); err != nil {
		t.Fatal(err)
	}

	req = httptest.NewRequest(http.MethodGet, cached.URL, nil)
	rec = httptest.NewRecorder()
	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("media status = %d", rec.Code)
	}
	if rec.Body.String() != "cached-video" {
		t.Fatalf("served body = %q", rec.Body.String())
	}
}

func TestCacheMediaRejectsVideosLargerThanRAMLimit(t *testing.T) {
	dir := t.TempDir()
	videoPath := filepath.Join(dir, "big.mp4")
	mustWrite(t, videoPath, "too-large")
	handler := NewWithConfig(testAssets(), "test", Config{RootDir: dir, Tools: media.Tools{}, MaxRAMCacheBytes: 3})
	entry := registerTestMedia(t, handler, videoPath)

	req := httptest.NewRequest(http.MethodPost, "/api/media/cache", strings.NewReader(`{"id":"`+entry.ID+`"}`))
	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusRequestEntityTooLarge {
		t.Fatalf("status = %d, body = %s", rec.Code, rec.Body.String())
	}
}

func registerTestMedia(t *testing.T, handler http.Handler, videoPath string) MediaEntry {
	t.Helper()
	body := bytes.NewBufferString(`{"path":"` + videoPath + `"}`)
	req := httptest.NewRequest(http.MethodPost, "/api/media/register", body)
	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("register status = %d, body = %s", rec.Code, rec.Body.String())
	}
	var entry MediaEntry
	if err := json.Unmarshal(rec.Body.Bytes(), &entry); err != nil {
		t.Fatal(err)
	}
	return entry
}

func mustWrite(t *testing.T, path, content string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}
}

func testAssets() fs.FS {
	return fstest.MapFS{
		"index.html": {Data: []byte("<html>ok</html>")},
	}
}
