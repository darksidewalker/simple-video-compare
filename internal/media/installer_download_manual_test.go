package media

import (
	"os"
	"path/filepath"
	"testing"
)

// Run with -tags=integ to exercise the real network download.
func TestDownloadStandaloneToolsInteg(t *testing.T) {
	if os.Getenv("DAWVC_FFMPEG_INTEG") != "1" {
		t.Skip("set DAWVC_FFMPEG_INTEG=1 to run the network download test")
	}
	dir := t.TempDir()
	d := NewDownloadInstaller(dir)
	tools, err := d.Download()
	if err != nil {
		t.Fatalf("download failed: %v", err)
	}
	if !tools.Ready {
		t.Fatalf("tools not ready after download: %+v", tools)
	}
	for name, tool := range map[string]Tool{"ffmpeg": tools.FFmpeg, "ffprobe": tools.FFprobe} {
		p := filepath.Join(dir, tool.Name)
		if _, err := os.Stat(p); err != nil {
			t.Fatalf("expected extracted %s at %s: %v", name, p, err)
		}
	}
}
