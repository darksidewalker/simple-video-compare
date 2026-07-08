package media

import "os/exec"

type Tool struct {
	Name      string `json:"name"`
	Path      string `json:"path"`
	Available bool   `json:"available"`
}

type Tools struct {
	FFmpeg  Tool `json:"ffmpeg"`
	FFprobe Tool `json:"ffprobe"`
	Ready   bool `json:"ready"`
}

type LookPathFunc func(string) (string, bool)

func ResolveTools(lookPath LookPathFunc) Tools {
	ffmpeg := resolveTool("ffmpeg", lookPath)
	ffprobe := resolveTool("ffprobe", lookPath)
	return Tools{FFmpeg: ffmpeg, FFprobe: ffprobe, Ready: ffmpeg.Available && ffprobe.Available}
}

func ResolvePathTools() Tools {
	return ResolveTools(func(name string) (string, bool) {
		path, err := exec.LookPath(name)
		return path, err == nil
	})
}

func resolveTool(name string, lookPath LookPathFunc) Tool {
	path, ok := lookPath(name)
	return Tool{Name: name, Path: path, Available: ok}
}
