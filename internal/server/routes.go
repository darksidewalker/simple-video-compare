package server

import "net/http"

func (s *Server) routes() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /health", s.handleHealth)
	mux.HandleFunc("GET /api/runtime", s.handleRuntime)
	mux.HandleFunc("GET /api/browse", s.handleBrowse)
	mux.HandleFunc("POST /api/video/probe", s.handleVideoProbe)
	mux.HandleFunc("POST /api/media/register", s.handleRegisterMedia)
	mux.HandleFunc("POST /api/media/cache", s.handleCacheMedia)
	mux.HandleFunc("POST /api/shutdown", HandleShutdown)
	mux.HandleFunc("GET /media/", s.handleMedia)
	mux.Handle("/", http.FileServer(http.FS(s.assets)))
	return mux
}
