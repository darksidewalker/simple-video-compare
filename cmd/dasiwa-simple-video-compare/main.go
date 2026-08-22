package main

import (
	"context"
	"embed"
	"flag"
	"fmt"
	"io/fs"
	"log"
	"net"
	"net/http"
	"os"
	"os/exec"
	"os/signal"
	"runtime"
	"strconv"
	"syscall"
	"time"

	"dasiwa-simple-video-compare/internal/media"
	"dasiwa-simple-video-compare/internal/server"
)

//go:embed web
var webFS embed.FS

func main() {
	noBrowser := flag.Bool("no-browser", false, "do not auto-open a browser window on start")
	flag.Parse()

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

	// Redirect stderr during server construction to suppress GTK/GDK protocol
	// errors from go-mpv's libmpv rendering backend. These occur because libmpv
	// is dynamically linked against GTK even when running headless (no window).
	stderrBackup := os.Stderr
	os.Stderr = os.NewFile(0, "/dev/null")
	srv := server.NewWithConfig(assets, "0.2.0-tauri", server.Config{
		RootDir:          rootDir,
		Tools:            media.ResolvePathTools(),
		MaxRAMCacheBytes: 2 << 30,
	})
	os.Stderr = stderrBackup

	// Bind the port ourselves so we know the server is accepting before we
	// point the browser at it. No WriteTimeout: the compatibility proxy runs
	// a synchronous ffmpeg transcode inside the register handler and serves
	// unbounded media, so a fixed write deadline would cut long responses.
	ln, err := net.Listen("tcp", fmt.Sprintf(":%d", port))
	if err != nil {
		log.Fatalf("server failed to bind :%d (is another instance already running?): %v", port, err)
	}

	httpServer := &http.Server{
		Handler:     srv,
		ReadTimeout: 30 * time.Second,
	}
	go httpServer.Serve(ln)

	url := fmt.Sprintf("http://127.0.0.1:%d", port)
	fmt.Printf("DaSiWa Simple Video Compare running at %s\n", url)

	if !*noBrowser && shouldOpenBrowser() {
		if err := launchBrowser(url); err != nil {
			log.Printf("could not auto-open a browser (%v); open %s manually", err, url)
		}
	} else {
		fmt.Printf("Open %s in a browser to use the app\n", url)
	}

	// Block until the process is asked to stop, then shut the server down
	// cleanly so in-flight media responses can finish.
	sig := make(chan os.Signal, 1)
	signal.Notify(sig, os.Interrupt, syscall.SIGTERM)
	<-sig

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	_ = httpServer.Shutdown(ctx)
}

// shouldOpenBrowser reports whether auto-opening a browser makes sense in the
// current environment. On Linux without a display (headless, bare daemon,
// systemd user unit with no session display) there is no window to open, so we
// stay quiet instead of firing a stray xdg-open.
func shouldOpenBrowser() bool {
	if runtime.GOOS == "linux" {
		if os.Getenv("DISPLAY") == "" && os.Getenv("WAYLAND_DISPLAY") == "" {
			return false
		}
	}
	return true
}

// launchBrowser opens the platform default browser at url without waiting for
// it to finish, so the caller can keep blocking on signals.
func launchBrowser(url string) error {
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "darwin":
		cmd = exec.Command("open", url)
	case "windows":
		// `start` treats the first quoted arg as a window title, hence the
		// empty-string placeholder before the URL.
		cmd = exec.Command("cmd", "/c", "start", "", url)
	default:
		cmd = exec.Command("xdg-open", url)
	}
	if err := cmd.Start(); err != nil {
		return err
	}
	return nil
}
