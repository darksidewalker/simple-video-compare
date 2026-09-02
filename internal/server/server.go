package server

import (
	"io/fs"
	"net/http"
	"os"
	"sync"
	"sync/atomic"

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
	// encoder is lazily resolved on first use (see encoder()). Once a GPU
	// encoder fails during a real transcode, encoderSoftFallback is set and
	// encoder() returns the software encoder for the rest of the run.
	encoder             media.Encoder
	encoderOnce         sync.Once
	encoderSoftFallback atomic.Bool
}

func New(assets fs.FS, version string) http.Handler {
	rootDir, err := os.UserHomeDir()
	if err != nil {
		rootDir = "."
	}
	return NewWithConfig(assets, version, Config{RootDir: rootDir, ToolsInfo: media.ResolvePathTools()})
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
