package main

import (
	"embed"
	"flag"
	"fmt"
	"io/fs"
	"log"
	"net/http"

	"dasiwa-simple-video-compare/internal/app"
	"dasiwa-simple-video-compare/internal/server"
)

var version = "dev"

//go:embed web
var webFS embed.FS

func main() {
	host := flag.String("host", "127.0.0.1", "server host")
	port := flag.Int("port", 8765, "server port")
	noOpen := flag.Bool("no-open", false, "do not open browser")
	browserMode := flag.Bool("browser", false, "open normal browser instead of local app window")
	flag.Parse()

	assets, err := fs.Sub(webFS, "web")
	if err != nil {
		log.Fatalf("web assets: %v", err)
	}

	addr := fmt.Sprintf("%s:%d", *host, *port)
	url := "http://" + addr
	if !*noOpen {
		go func() {
			if *browserMode {
				_ = app.OpenBrowser(url)
				return
			}
			_ = app.OpenAppWindow(url)
		}()
	}

	fmt.Printf("DaSiWa Simple Video Compare running at %s\n", url)
	if err := http.ListenAndServe(addr, server.New(assets, version)); err != nil {
		log.Fatal(err)
	}
}
