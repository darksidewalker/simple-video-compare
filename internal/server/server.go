package server

import (
	"io/fs"
	"net/http"
	"os"
	"sync"

	"dasiwa-simple-video-compare/internal/media"
)

type Server struct {
	assets  fs.FS
	version string
	config  Config
	mu      sync.RWMutex
	media   map[string]MediaEntry
	cache   map[string][]byte
	compat  map[string]string
}

func New(assets fs.FS, version string) http.Handler {
	rootDir, err := os.UserHomeDir()
	if err != nil {
		rootDir = "."
	}
	return NewWithConfig(assets, version, Config{RootDir: rootDir, Tools: media.ResolvePathTools()})
}

func NewWithConfig(assets fs.FS, version string, config Config) http.Handler {
	if config.RootDir == "" {
		config.RootDir = "."
	}
	if config.MaxRAMCacheBytes == 0 {
		config.MaxRAMCacheBytes = 2 << 30
	}
	s := &Server{assets: assets, version: version, config: config, media: make(map[string]MediaEntry), cache: make(map[string][]byte), compat: make(map[string]string)}
	return s.routes()
}
