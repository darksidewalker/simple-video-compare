package server

import (
	"bytes"
	"crypto/sha1"
	"encoding/hex"
	"encoding/json"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"
)

type registerMediaRequest struct {
	Path string `json:"path"`
}

type cacheMediaRequest struct {
	ID string `json:"id"`
}

func (s *Server) handleRegisterMedia(w http.ResponseWriter, r *http.Request) {
	var req registerMediaRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]string{"error": "invalid json"})
		return
	}
	path := cleanPath(req.Path, "")
	info, err := os.Stat(path)
	if err != nil || info.IsDir() {
		writeJSON(w, http.StatusBadRequest, map[string]string{"error": "video file not found"})
		return
	}
	if !isVideoFile(path) {
		writeJSON(w, http.StatusBadRequest, map[string]string{"error": "unsupported video extension"})
		return
	}

	entry := MediaEntry{ID: mediaID(path), Name: filepath.Base(path), Path: path}
	entry.URL = "/media/" + entry.ID + "/" + entry.Name
	s.mu.Lock()
	s.media[entry.ID] = entry
	s.mu.Unlock()
	writeJSON(w, http.StatusOK, entry)
}

func (s *Server) handleMedia(w http.ResponseWriter, r *http.Request) {
	id := mediaIDFromURL(r.URL.Path)
	if id == "" {
		http.NotFound(w, r)
		return
	}
	s.mu.RLock()
	entry, ok := s.media[id]
	s.mu.RUnlock()
	if !ok {
		http.NotFound(w, r)
		return
	}
	if cached, ok := s.cache[id]; ok {
		http.ServeContent(w, r, entry.Name, time.Now(), bytes.NewReader(cached))
		return
	}
	http.ServeFile(w, r, entry.Path)
}

func (s *Server) handleCacheMedia(w http.ResponseWriter, r *http.Request) {
	var req cacheMediaRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || req.ID == "" {
		writeJSON(w, http.StatusBadRequest, map[string]string{"error": "invalid json"})
		return
	}

	s.mu.RLock()
	entry, ok := s.media[req.ID]
	s.mu.RUnlock()
	if !ok {
		http.NotFound(w, r)
		return
	}

	info, err := os.Stat(entry.Path)
	if err != nil || info.IsDir() {
		writeJSON(w, http.StatusBadRequest, map[string]string{"error": "video file not found"})
		return
	}
	if info.Size() > s.config.MaxRAMCacheBytes {
		writeJSON(w, http.StatusRequestEntityTooLarge, map[string]any{"error": "video exceeds RAM cache limit", "size": info.Size(), "limit": s.config.MaxRAMCacheBytes})
		return
	}

	file, err := os.Open(entry.Path)
	if err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]string{"error": err.Error()})
		return
	}
	defer file.Close()
	data, err := io.ReadAll(io.LimitReader(file, s.config.MaxRAMCacheBytes+1))
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]string{"error": err.Error()})
		return
	}
	if int64(len(data)) > s.config.MaxRAMCacheBytes {
		writeJSON(w, http.StatusRequestEntityTooLarge, map[string]any{"error": "video exceeds RAM cache limit", "size": len(data), "limit": s.config.MaxRAMCacheBytes})
		return
	}

	entry.Cached = true
	entry.CacheBytes = int64(len(data))
	s.mu.Lock()
	s.cache[entry.ID] = data
	s.media[entry.ID] = entry
	s.mu.Unlock()
	writeJSON(w, http.StatusOK, entry)
}

func mediaID(path string) string {
	sum := sha1.Sum([]byte(path))
	return hex.EncodeToString(sum[:])[:16]
}

func mediaIDFromURL(path string) string {
	rel := strings.TrimPrefix(path, "/media/")
	parts := strings.SplitN(rel, "/", 2)
	if len(parts) == 0 || strings.Contains(parts[0], "..") {
		return ""
	}
	return parts[0]
}
