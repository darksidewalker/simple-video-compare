package media

import (
	"archive/tar"
	"archive/zip"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"time"

	"github.com/ulikunitz/xz"
)

// defaultDownloadBase is the floating "latest" release of BtbN/FFmpeg-Builds.
// The URL stays stable while always pointing at the newest static build. It
// ships static x86_64 builds for Linux and Windows only, so macOS users get a
// manual-install suggestion instead of a download.
const defaultDownloadBase = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/"

// DownloadInstaller downloads and verifies standalone ffmpeg/ffprobe builds
// into an app-local directory. Tool resolution order is app-local dir first,
// then the system PATH, so an existing system install always wins.
type DownloadInstaller struct {
	appLocalDir string
	DownloadURL string
	ManualHint  string
	httpClient  *http.Client

	mu     sync.RWMutex
	tools  Tools
	loaded bool
}

// NewDownloadInstaller builds an installer for appLocalDir; download URL and
// manual hint default to the platform-appropriate values.
func NewDownloadInstaller(appLocalDir string) *DownloadInstaller {
	d := &DownloadInstaller{
		appLocalDir: appLocalDir,
		ManualHint:  manualInstallHint(runtime.GOOS),
		httpClient:  &http.Client{Timeout: 5 * time.Minute},
	}
	if name := defaultDownloadName(runtime.GOOS); name != "" {
		d.DownloadURL = defaultDownloadBase + name
	}
	return d
}

func defaultDownloadName(goos string) string {
	switch goos {
	case "windows":
		return "ffmpeg-master-latest-win64-gpl.zip"
	case "linux":
		return "ffmpeg-master-latest-linux64-gpl.tar.xz"
	default:
		return ""
	}
}

func manualInstallHint(goos string) string {
	switch goos {
	case "windows":
		return "Download a standalone build (e.g. gyan.dev or BtbN/FFmpeg-Builds) and place ffmpeg.exe and ffprobe.exe into the app folder listed above."
	case "darwin":
		return "Install via `brew install ffmpeg` or place ffmpeg/ffprobe into a directory on your PATH."
	default:
		return "Install via your package manager (e.g. `sudo apt install ffmpeg`) or place ffmpeg/ffprobe into a directory on your PATH."
	}
}

// Dir returns the app-local directory where downloaded binaries are kept.
func (d *DownloadInstaller) Dir() string { return d.appLocalDir }

// Manual returns the platform-specific hint shown when the user chooses to
// install on their own.
func (d *DownloadInstaller) Manual() string { return d.ManualHint }

// Tools resolves ffmpeg/ffprobe: env override first, then the app-local
// directory, then the system PATH. Once both tools are available the result
// is cached; while tools are missing a fresh filesystem check is re-run so
// binaries the user installed manually are picked up without a restart.
func (d *DownloadInstaller) Tools() Tools {
	d.mu.RLock()
	if d.loaded && d.tools.Ready {
		t := d.tools
		d.mu.RUnlock()
		return t
	}
	d.mu.RUnlock()
	d.mu.Lock()
	defer d.mu.Unlock()
	d.tools = d.resolveLocked()
	d.loaded = true
	return d.tools
}

// Verify re-runs resolution, picking up binaries the user placed manually
// since the app started.
func (d *DownloadInstaller) Verify() Tools {
	d.mu.Lock()
	defer d.mu.Unlock()
	d.tools = d.resolveLocked()
	d.loaded = true
	return d.tools
}

// resolveLocked resolves ffmpeg/ffprobe. An explicit DAWVC_FFMPEG/DAWVC_FFPROBE
// environment variable wins over everything (explicit user choice), then the
// app-local directory (downloaded standalone builds), then the system PATH.
func (d *DownloadInstaller) resolveLocked() Tools {
	ffmpeg := resolveFromEnv("DAWVC_FFMPEG", "ffmpeg")
	if !ffmpeg.Available {
		ffmpeg = resolveFromDir("ffmpeg", d.appLocalDir, pathExt())
	}
	if !ffmpeg.Available {
		ffmpeg = resolveFromPath("ffmpeg")
	}
	ffprobe := resolveFromEnv("DAWVC_FFPROBE", "ffprobe")
	if !ffprobe.Available {
		ffprobe = resolveFromDir("ffprobe", d.appLocalDir, pathExt())
	}
	if !ffprobe.Available {
		ffprobe = resolveFromPath("ffprobe")
	}
	return Tools{FFmpeg: ffmpeg, FFprobe: ffprobe, Ready: ffmpeg.Available && ffprobe.Available}
}

// resolveFromEnv reads an environment variable that may hold either an
// absolute path to the binary or a bare command name (resolved via PATH).
func resolveFromEnv(envKey, name string) Tool {
	v := strings.TrimSpace(os.Getenv(envKey))
	if v == "" {
		return Tool{Name: name}
	}
	if strings.ContainsAny(v, "/\\") {
		// Absolute (or relative) path: trust it directly if it exists.
		if st, err := os.Stat(v); err == nil && !st.IsDir() {
			return Tool{Name: name, Path: v, Available: true}
		}
		return Tool{Name: name}
	}
	// Bare name: resolve through PATH/PATHEXT.
	if p, err := exec.LookPath(v); err == nil {
		return Tool{Name: name, Path: p, Available: true}
	}
	return Tool{Name: name}
}

// pathExt returns the executable suffix used by the platform. On Windows
// exec.LookPath finds "ffmpeg.exe" via PATHEXT, but a bare name inside an
// app-local directory has to be spelled with the extension explicitly.
func pathExt() string {
	if runtime.GOOS == "windows" {
		return ".exe"
	}
	return ""
}

func resolveFromDir(name, dir, ext string) Tool {
	if dir == "" {
		return Tool{Name: name}
	}
	p := filepath.Join(dir, name+ext)
	if st, err := os.Stat(p); err == nil && !st.IsDir() {
		return Tool{Name: name, Path: p, Available: true}
	}
	return Tool{Name: name}
}

func resolveFromPath(name string) Tool {
	p, err := exec.LookPath(name)
	if err != nil {
		return Tool{Name: name}
	}
	return Tool{Name: name, Path: p, Available: true}
}

// Download fetches the pinned static build, extracts ffmpeg and ffprobe into
// Dir() and re-resolves tools. It is idempotent: existing files are kept.
func (d *DownloadInstaller) Download() (Tools, error) {
	if d.DownloadURL == "" {
		return d.Tools(), errors.New("no standalone download available on this platform; install manually")
	}
	if err := os.MkdirAll(d.appLocalDir, 0o755); err != nil {
		return d.Tools(), fmt.Errorf("create app tools dir: %w", err)
	}

	tmp, err := os.CreateTemp("", "dasiwa-ffmpeg-*")
	if err != nil {
		return d.Tools(), fmt.Errorf("create temp file: %w", err)
	}
	tmpPath := tmp.Name()
	defer os.Remove(tmpPath)

	req, err := http.NewRequest(http.MethodGet, d.DownloadURL, nil)
	if err != nil {
		tmp.Close()
		return d.Tools(), fmt.Errorf("build download request: %w", err)
	}
	resp, err := d.httpClient.Do(req)
	if err != nil {
		tmp.Close()
		return d.Tools(), fmt.Errorf("download failed: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		tmp.Close()
		return d.Tools(), fmt.Errorf("download failed: HTTP %d", resp.StatusCode)
	}
	if _, err := io.Copy(tmp, resp.Body); err != nil {
		tmp.Close()
		return d.Tools(), fmt.Errorf("download failed: %w", err)
	}
	if err := tmp.Close(); err != nil {
		return d.Tools(), fmt.Errorf("download failed: %w", err)
	}

	switch path.Ext(d.DownloadURL) {
	case ".zip":
		if err := extractZip(tmpPath, d.appLocalDir); err != nil {
			return d.Tools(), err
		}
	default:
		if err := extractTarXz(tmpPath, d.appLocalDir); err != nil {
			return d.Tools(), err
		}
	}

	return d.Verify(), nil
}

// zip archives keep the binaries in a "<name>/bin/" folder at the root.
func extractZip(src, dest string) error {
	zr, err := zip.OpenReader(src)
	if err != nil {
		return fmt.Errorf("open archive: %w", err)
	}
	defer zr.Close()
	want := []string{"ffmpeg", "ffprobe"}
	for _, f := range zr.File {
		rel, ok := wantedEntry(f.Name, want)
		if !ok {
			continue
		}
		rc, err := f.Open()
		if err != nil {
			return fmt.Errorf("open entry %q: %w", f.Name, err)
		}
		if err := writeEntry(dest, rel, rc); err != nil {
			rc.Close()
			return err
		}
		rc.Close()
	}
	return nil
}

// tar.xz archives keep the binaries in a "<name>/bin/" folder at the root.
func extractTarXz(src, dest string) error {
	f, err := os.Open(src)
	if err != nil {
		return fmt.Errorf("open archive: %w", err)
	}
	defer f.Close()
	xzR, err := xz.NewReader(f)
	if err != nil {
		return fmt.Errorf("decompress archive: %w", err)
	}
	tr := tar.NewReader(xzR)
	want := []string{"ffmpeg", "ffprobe"}
	for {
		hdr, err := tr.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			return fmt.Errorf("read archive: %w", err)
		}
		if hdr.Typeflag != tar.TypeReg {
			continue
		}
		rel, ok := wantedEntry(hdr.Name, want)
		if !ok {
			continue
		}
		if err := writeEntry(dest, rel, tr); err != nil {
			return err
		}
	}
	return nil
}

// wantedEntry finds the requested tool binary inside the archive layout and
// returns the bare tool name plus whether the entry matches.
func wantedEntry(name string, want []string) (string, bool) {
	_, base := path.Split(name)
	for _, w := range want {
		if base == w || base == w+".exe" {
			return w, true
		}
	}
	return "", false
}

// writeEntry extracts one archive entry (an io.Reader) into dest/<name>,
// rejecting entries whose path would escape the destination directory.
func writeEntry(dest, name string, r io.Reader) error {
	target := filepath.Join(dest, filepath.FromSlash(name))
	rel, err := filepath.Rel(dest, target)
	if err != nil || rel == ".." || rel == ".."+string(filepath.Separator) {
		return fmt.Errorf("refusing to extract outside target: %q", name)
	}
	out, err := os.OpenFile(target, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o755)
	if err != nil {
		return fmt.Errorf("write %q: %w", name, err)
	}
	defer out.Close()
	if _, err := io.Copy(out, r); err != nil {
		return fmt.Errorf("write %q: %w", name, err)
	}
	return nil
}
