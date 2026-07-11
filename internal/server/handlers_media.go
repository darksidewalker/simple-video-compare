package server

import (
	"bytes"
	"crypto/sha1"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"os/exec"
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

	entry := MediaEntry{ID: mediaID(path), Name: filepath.Base(path), Path: path, Compatible: true}
	if s.needsCompatibleProxy(path) {
		compatiblePath, err := s.ensureCompatibleMedia(entry.ID, path)
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, map[string]string{"error": err.Error()})
			return
		}
		entry.Path = compatiblePath
		entry.Name = filepath.Base(compatiblePath)
		entry.Compatible = false
	}
	entry.URL = "/media/" + entry.ID + "/" + url.PathEscape(entry.Name)
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

func (s *Server) needsCompatibleProxy(path string) bool {
	if !s.config.Tools.FFprobe.Available || s.config.Tools.FFprobe.Path == "" || !s.config.Tools.FFmpeg.Available || s.config.Tools.FFmpeg.Path == "" {
		return false
	}
	cmd := exec.Command(s.config.Tools.FFprobe.Path, "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=codec_name", "-of", "default=noprint_wrappers=1:nokey=1", path)
	output, err := cmd.Output()
	if err != nil {
		return false
	}
	codec := strings.TrimSpace(string(output))
	return codec == "hevc" || codec == "h265" || codec == "vp9" || codec == "av1"
}

func (s *Server) ensureCompatibleMedia(id, sourcePath string) (string, error) {
	s.mu.RLock()
	if path, ok := s.compat[id]; ok {
		s.mu.RUnlock()
		if _, err := os.Stat(path); err == nil {
			return path, nil
		}
	} else {
		s.mu.RUnlock()
	}

	outPath := filepath.Join(os.TempDir(), "dasiwa-simple-video-compare", id+"-h264.mp4")
	if err := os.MkdirAll(filepath.Dir(outPath), 0755); err != nil {
		return "", err
	}
	if _, err := os.Stat(outPath); err == nil {
		s.mu.Lock()
		s.compat[id] = outPath
		s.mu.Unlock()
		return outPath, nil
	}

	cmd := exec.Command(s.config.Tools.FFmpeg.Path,
		"-y", "-i", sourcePath,
		"-map", "0:v:0", "-map", "0:a?",
		"-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p",
		"-c:a", "aac", "-b:a", "160k",
		"-movflags", "+faststart",
		outPath,
	)
	if output, err := cmd.CombinedOutput(); err != nil {
		return "", fmt.Errorf("ffmpeg compatibility transcode failed: %v: %s", err, strings.TrimSpace(string(output)))
	}

	s.mu.Lock()
	s.compat[id] = outPath
	s.mu.Unlock()
	return outPath, nil
}

func mediaIDFromURL(path string) string {
	rel := strings.TrimPrefix(path, "/media/")
	parts := strings.SplitN(rel, "/", 2)
	if len(parts) == 0 || strings.Contains(parts[0], "..") {
		return ""
	}
	return parts[0]
}
