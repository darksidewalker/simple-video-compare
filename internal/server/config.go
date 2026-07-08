package server

import "dasiwa-simple-video-compare/internal/media"

type Config struct {
	RootDir          string
	Tools            media.Tools
	MaxRAMCacheBytes int64
}

type MediaEntry struct {
	ID         string `json:"id"`
	Name       string `json:"name"`
	Path       string `json:"path"`
	URL        string `json:"url"`
	Cached     bool   `json:"cached"`
	CacheBytes int64  `json:"cache_bytes"`
}

type RuntimeInfo struct {
	Status  string      `json:"status"`
	App     string      `json:"app"`
	Version string      `json:"version"`
	UXMode  string      `json:"ux_mode"`
	RootDir string      `json:"root_dir"`
	Tools   media.Tools `json:"tools"`
}
