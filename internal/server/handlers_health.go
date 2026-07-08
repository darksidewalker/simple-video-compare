package server

import "net/http"

func (s *Server) handleHealth(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, http.StatusOK, map[string]string{
		"status":  "ok",
		"app":     "DaSiWa Simple Video Compare",
		"version": s.version,
	})
}

func (s *Server) handleRuntime(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, http.StatusOK, RuntimeInfo{
		Status:  "ok",
		App:     "DaSiWa Simple Video Compare",
		Version: s.version,
		UXMode:  "local-app-window",
		RootDir: s.config.RootDir,
		Tools:   s.config.Tools,
	})
}
