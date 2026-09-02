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

	"dasiwa-simple-video-compare/internal/media"
)

type registerMediaRequest struct {
	// Path is the source file on disk. Capabilities lists the codecs the
	// requesting client can decode natively (e.g. "av1", "hevc"); when the
	// source codec is in this list the server serves it directly and skips
	// the H.264 proxy entirely.
	Path         string   `json:"path"`
	Capabilities []string `json:"capabilities,omitempty"`
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
	if s.needsCompatibleProxyFor(path, req.Capabilities) {
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

// probeFirstVideoCodec runs ffprobe and returns the codec name of the first
// video stream. JSON output is used because the flat default format can emit
// the codec line more than once for certain container files, which broke the
// exact-string comparison the compatibility proxy relies on.
func (s *Server) probeFirstVideoCodec(path string) string {
	cmd := exec.Command(s.config.Tools().FFprobe.Path, "-v", "error", "-select_streams", "v:0", "-show_streams", "-of", "json", path)
	output, err := cmd.Output()
	if err != nil {
		return ""
	}
	var parsed struct {
		Streams []struct {
			CodecType string `json:"codec_type"`
			CodecName string `json:"codec_name"`
		} `json:"streams"`
	}
	if err := json.Unmarshal(output, &parsed); err != nil {
		return ""
	}
	for _, stream := range parsed.Streams {
		if stream.CodecType == "video" {
			return stream.CodecName
		}
	}
	return ""
}

func (s *Server) needsCompatibleProxy(path string) bool {
	return s.needsCompatibleProxyFor(path, nil)
}

// needsCompatibleProxyFor reports whether the source file must be transcoded
// to the browser-friendly H.264 proxy. It returns true only when the source's
// first video codec is one browsers often cannot handle natively (HEVC, VP9,
// AV1) AND the client did not report that it can decode that codec on its own.
// When the client declares native support for the source codec we serve the
// file directly and skip the transcode.
func (s *Server) needsCompatibleProxyFor(path string, clientCaps []string) bool {
	tools := s.config.Tools()
	if !tools.FFprobe.Available || tools.FFprobe.Path == "" || !tools.FFmpeg.Available || tools.FFmpeg.Path == "" {
		return false
	}
	codec := strings.ToLower(strings.TrimSpace(s.probeFirstVideoCodec(path)))
	if !isBrowserHostileCodec(codec) {
		return false
	}
	for _, cap := range clientCaps {
		if strings.EqualFold(strings.TrimSpace(cap), codec) {
			return false
		}
	}
	return true
}

// isBrowserHostileCodec lists video codecs that most browser <video> elements
// cannot decode without native hardware/software support. H.264 is deliberately
// omitted: it is the universal baseline and the proxy target.
func isBrowserHostileCodec(codec string) bool {
	switch codec {
	case "hevc", "h265", "vp9", "av1":
		return true
	}
	return false
}

// encoder returns the video encoder to use for compatibility proxies. The
// first call resolves it via media.DetectEncoder (honoring DAWVC_ENCODER and
// probing for a working GPU encoder); the result is cached for the process.
// Once markEncoderFallback has fired (a GPU encoder failed during a real
// transcode), software is returned for the rest of the run.
func (s *Server) videoEncoder() media.Encoder {
	s.encoderOnce.Do(func() {
		s.encoder = media.DetectEncoder(s.config.Tools().FFmpeg.Path)
	})
	if s.encoderSoftFallback.Load() {
		return media.SoftwareEncoder()
	}
	return s.encoder
}

// markEncoderFallback switches the proxy to software after a GPU encoder
// failed during a real transcode (e.g. the card was busy with another process
// at that instant). DAWVC_ENCODER stays authoritative: if the user pinned an
// encoder we do not second-guess it and say nothing.
func (s *Server) markEncoderFallback() {
	if s.encoderSoftFallback.Swap(true) {
		return
	}
	if strings.TrimSpace(os.Getenv("DAWVC_ENCODER")) == "" {
		fmt.Fprintln(os.Stderr, "dasiwa: hardware encoder failed, falling back to software (libx264)")
	}
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

	enc := s.videoEncoder()
	args := []string{
		"-y", "-i", sourcePath,
		"-map", "0:v:0", "-map", "0:a?",
		"-c:v", enc.Name,
	}
	args = append(args, enc.Args...)
	args = append(args,
		"-pix_fmt", "yuv420p",
		"-c:a", "aac", "-b:a", "160k",
		"-movflags", "+faststart",
		outPath,
	)
	if output, err := exec.Command(s.config.Tools().FFmpeg.Path, args...).CombinedOutput(); err != nil {
		// A one-time fallback to software so a single GPU hiccup does not
		// break the whole session.
		if enc.Name != media.SoftwareEncoder().Name {
			s.markEncoderFallback()
			args2 := make([]string, 0, len(args)+2)
			args2 = append(args2, "-y", "-i", sourcePath, "-map", "0:v:0", "-map", "0:a?")
			soft := media.SoftwareEncoder()
			args2 = append(args2, "-c:v", soft.Name)
			args2 = append(args2, soft.Args...)
			args2 = append(args2, "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "160k", "-movflags", "+faststart", outPath)
			if output2, err2 := exec.Command(s.config.Tools().FFmpeg.Path, args2...).CombinedOutput(); err2 != nil {
				return "", fmt.Errorf("ffmpeg compatibility transcode failed (hw %s: %v: %s; sw %v: %s)", enc.Name, err, strings.TrimSpace(string(output)), err2, strings.TrimSpace(string(output2)))
			}
		} else {
			return "", fmt.Errorf("ffmpeg compatibility transcode failed: %v: %s", err, strings.TrimSpace(string(output)))
		}
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
