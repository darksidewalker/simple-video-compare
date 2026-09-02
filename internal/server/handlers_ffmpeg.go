package server

import "net/http"

// handleFFmpegInstall downloads the pinned standalone ffmpeg/ffprobe build
// into the app-local directory and re-resolves tools so the app can use them
// immediately, without a restart.
func (s *Server) handleFFmpegInstall(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}
	if s.config.Installer == nil {
		writeJSON(w, http.StatusNotImplemented, map[string]string{"error": "ffmpeg installer not available in this build"})
		return
	}
	tools, err := s.config.Installer.Download()
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]any{
			"error":  err.Error(),
			"tools":  tools,
			"manual": s.config.Installer.Manual(),
		})
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{
		"tools": tools,
		"dir":   s.config.Installer.Dir(),
	})
}

// handleFFmpegVerify re-checks PATH and the app-local directory for
// ffmpeg/ffprobe, picking up binaries the user installed manually since the
// app started.
func (s *Server) handleFFmpegVerify(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}
	tools := s.config.Tools()
	writeJSON(w, http.StatusOK, map[string]any{
		"tools": tools,
	})
}
