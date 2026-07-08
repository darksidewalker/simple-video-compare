package server

import (
	"net/http"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

type BrowseItem struct {
	Name  string `json:"name"`
	Path  string `json:"path"`
	IsDir bool   `json:"is_dir"`
}

func (s *Server) handleBrowse(w http.ResponseWriter, r *http.Request) {
	path := cleanPath(r.URL.Query().Get("path"), s.config.RootDir)
	entries, err := os.ReadDir(path)
	if err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]string{"error": err.Error()})
		return
	}

	items := make([]BrowseItem, 0, len(entries))
	for _, entry := range entries {
		if strings.HasPrefix(entry.Name(), ".") {
			continue
		}
		if !entry.IsDir() && !isVideoFile(entry.Name()) {
			continue
		}
		items = append(items, BrowseItem{
			Name:  entry.Name(),
			Path:  filepath.Join(path, entry.Name()),
			IsDir: entry.IsDir(),
		})
	}
	sort.Slice(items, func(i, j int) bool {
		if items[i].IsDir != items[j].IsDir {
			return items[i].IsDir
		}
		return strings.ToLower(items[i].Name) < strings.ToLower(items[j].Name)
	})

	writeJSON(w, http.StatusOK, map[string]any{
		"path":   path,
		"parent": filepath.Dir(path),
		"items":  items,
	})
}

func cleanPath(value, fallback string) string {
	if value == "" {
		value = fallback
	}
	if strings.HasPrefix(value, "~") {
		if home, err := os.UserHomeDir(); err == nil {
			value = filepath.Join(home, strings.TrimPrefix(value, "~"))
		}
	}
	if abs, err := filepath.Abs(value); err == nil {
		value = abs
	}
	if real, err := filepath.EvalSymlinks(value); err == nil {
		value = real
	}
	return value
}

func isVideoFile(name string) bool {
	switch strings.ToLower(filepath.Ext(name)) {
	case ".mp4", ".mkv", ".mov", ".webm", ".avi", ".m4v", ".mpg", ".mpeg", ".ts", ".m2ts", ".flv":
		return true
	default:
		return false
	}
}
