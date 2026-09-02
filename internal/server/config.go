package server

import "dasiwa-simple-video-compare/internal/media"

// Config is the runtime configuration for the local HTTP server.
type Config struct {
	RootDir string
	// ToolsInfo is the tool state resolved at startup (PATH lookup only).
	// When an installer is set it is kept for reference; live checks use
	// Tools(), which prefers freshly installed app-local binaries.
	ToolsInfo media.Tools
	// Installer, when set, resolves ffmpeg/ffprobe from the app-local
	// directory first and then the system PATH, and can download standalone
	// builds.
	Installer        *media.DownloadInstaller
	MaxRAMCacheBytes int64
}

// Tools resolves ffmpeg/ffprobe. When an installer is set it re-resolves
// (app-local dir first, then PATH) so freshly installed binaries are picked
// up without a restart; otherwise the startup tools are returned.
func (c Config) Tools() media.Tools {
	if c.Installer != nil {
		return c.Installer.Tools()
	}
	return c.ToolsInfo
}

type MediaEntry struct {
	ID         string `json:"id"`
	Name       string `json:"name"`
	Path       string `json:"path"`
	URL        string `json:"url"`
	Cached     bool   `json:"cached"`
	CacheBytes int64  `json:"cache_bytes"`
	Compatible bool   `json:"compatible"`
}

// RuntimeInfo describes the runtime environment of the local server.
type RuntimeInfo struct {
	Status      string      `json:"status"`
	App         string      `json:"app"`
	Version     string      `json:"version"`
	UXMode      string      `json:"ux_mode"`
	RootDir     string      `json:"root_dir,omitempty"`
	Tools       media.Tools `json:"tools"`
	FFMpegDir   string      `json:"ffmpeg_dir,omitempty"`
	Manual      string      `json:"ffmpeg_manual,omitempty"`
	DownloadURL string      `json:"ffmpeg_download,omitempty"`
}
