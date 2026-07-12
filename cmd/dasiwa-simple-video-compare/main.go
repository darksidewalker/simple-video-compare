package main

import (
	"embed"
	"fmt"
	"io/fs"
	"log"
	"net/http"
	"os"
	"strconv"
	"time"

	"dasiwa-simple-video-compare/internal/media"
	"dasiwa-simple-video-compare/internal/server"
)

//go:embed web
var webFS embed.FS

func main() {
	port := 1420
	if p := os.Getenv("DAWVC_PORT"); p != "" {
		if v, err := strconv.Atoi(p); err == nil && v > 0 {
			port = v
		}
	}

	assets, err := fs.Sub(webFS, "web")
	if err != nil {
		log.Fatalf("failed to embed web assets: %v", err)
	}

	rootDir, _ := os.UserHomeDir()
	if rootDir == "" {
		rootDir = "."
	}

	srv := server.NewWithConfig(assets, "0.2.0-tauri", server.Config{
		RootDir:          rootDir,
		Tools:            media.ResolvePathTools(),
		MaxRAMCacheBytes: 2 << 30,
	})

	addr := fmt.Sprintf(":%d", port)
	httpServer := &http.Server{
		Addr:         addr,
		Handler:      srv,
		ReadTimeout:  30 * time.Second,
		WriteTimeout: 30 * time.Second,
	}

	fmt.Printf("DaSiWa Simple Video Compare starting on http://localhost%s\n", addr)
	if err := httpServer.ListenAndServe(); err != nil && err != http.ErrServerClosed {
		log.Fatalf("server failed: %v", err)
	}
}
