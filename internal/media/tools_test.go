package media

import "testing"

func TestResolveToolFindsFFmpegAndFFprobe(t *testing.T) {
	tools := ResolveTools(func(name string) (string, bool) {
		switch name {
		case "ffmpeg":
			return "/usr/bin/ffmpeg", true
		case "ffprobe":
			return "/usr/bin/ffprobe", true
		default:
			return "", false
		}
	})

	if !tools.FFmpeg.Available || tools.FFmpeg.Path != "/usr/bin/ffmpeg" {
		t.Fatalf("unexpected ffmpeg info: %+v", tools.FFmpeg)
	}
	if !tools.FFprobe.Available || tools.FFprobe.Path != "/usr/bin/ffprobe" {
		t.Fatalf("unexpected ffprobe info: %+v", tools.FFprobe)
	}
	if !tools.Ready {
		t.Fatal("expected media runtime to be ready")
	}
}
