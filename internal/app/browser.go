package app

import (
	"fmt"
	"os/exec"
	"runtime"
)

type lookPathFunc func(string) (string, bool)

func OpenBrowser(url string) error {
	cmd, args, err := browserCommand(url)
	if err != nil {
		return err
	}
	return exec.Command(cmd, args...).Start()
}

func OpenAppWindow(url string) error {
	cmd, args, ok := appWindowCommand(url, func(name string) (string, bool) {
		path, err := exec.LookPath(name)
		return path, err == nil
	})
	if !ok {
		return OpenBrowser(url)
	}
	return exec.Command(cmd, args...).Start()
}

func appWindowCommand(url string, lookPath lookPathFunc) (string, []string, bool) {
	if runtime.GOOS != "linux" {
		return "", nil, false
	}
	for _, name := range []string{"chromium", "chromium-browser", "google-chrome", "brave-browser", "microsoft-edge"} {
		path, ok := lookPath(name)
		if ok {
			return path, []string{"--new-window", "--app=" + url}, true
		}
	}
	return "", nil, false
}

func browserCommand(url string) (string, []string, error) {
	switch runtime.GOOS {
	case "linux":
		return "xdg-open", []string{url}, nil
	case "darwin":
		return "open", []string{url}, nil
	case "windows":
		return "cmd", []string{"/c", "start", "", url}, nil
	default:
		return "", nil, fmt.Errorf("unsupported OS: %s", runtime.GOOS)
	}
}
