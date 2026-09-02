package media

import (
	"os"
	"os/exec"
	"strings"
)

// Encoder describes one video encoder candidate for the compatibility proxy.
// Name is the ffmpeg encoder name; Args are the encoder options that best
// approximate the previous software settings (-preset veryfast -crf 18).
// Label is a human-readable name for the UI ("NVIDIA NVENC", ...).
type Encoder struct {
	Name  string
	Args  []string
	Label string
}

// SoftwareEncoder is the guaranteed fallback; it always works.
func SoftwareEncoder() Encoder {
	c := CandidateEncoders()
	return c[len(c)-1]
}

// CandidateEncoders returns the acceleration candidates in priority order:
// NVIDIA first (most common in this project's user base), then AMD, then
// Intel, then the Linux-only VAAPI, and finally pure software as the last
// resort. The last entry is always the software encoder.
func CandidateEncoders() []Encoder {
	return []Encoder{
		{Name: "h264_nvenc", Args: []string{"-preset", "p4", "-tune", "hq", "-rc", "vbr", "-cq", "18"}, Label: "NVIDIA NVENC"},
		{Name: "h264_amf", Args: []string{"-quality", "balanced", "-rc-mode", "cqp", "-quantizer", "18"}, Label: "AMD AMF"},
		{Name: "h264_qsv", Args: []string{"-preset", "mediumspeed", "-global_quality", "18"}, Label: "Intel QSV"},
		{Name: "h264_vaapi", Args: []string{"-qp", "18"}, Label: "VAAPI (Linux)"},
		{Name: "libx264", Args: []string{"-preset", "veryfast", "-crf", "18"}, Label: "software (libx264)"},
	}
}

// DetectEncoder returns the best working encoder for the given ffmpeg binary.
// It runs a near-zero-cost capability probe: a single 64x64 frame from a lavfi
// test source through each candidate, in priority order. GPU encoders fail
// fast (exit non-zero) when no matching hardware or driver is present, so the
// first candidate that encodes successfully wins. The software encoder is
// tried last and always succeeds, so this function never returns empty.
//
// The DAWVC_ENCODER environment variable overrides detection: a recognized
// shorthand (nvenc|amf|qsv|vaapi|x264, or vendor aliases nvidia|amd|intel|
// software) selects that encoder directly, and an unrecognized value is probed
// verbatim before falling back to auto-detection.
func DetectEncoder(ffmpegPath string) Encoder {
	if v := strings.TrimSpace(os.Getenv("DAWVC_ENCODER")); v != "" {
		if enc, ok := findEncoder(v); ok {
			return enc
		}
		// Unknown override: try it verbatim; if it does not work here, fall
		// through to normal detection so we still get a working encoder.
		verbatim := Encoder{Name: v, Args: argsFor(v), Label: "custom (" + v + ")"}
		if probeEncoder(ffmpegPath, verbatim) {
			return verbatim
		}
	}
	for _, enc := range CandidateEncoders() {
		if probeEncoder(ffmpegPath, enc) {
			return enc
		}
	}
	return SoftwareEncoder()
}

// findEncoder maps a DAWVC_ENCODER shorthand to a candidate. Vendor aliases
// (nvidia/amd/intel) resolve to the matching encoder so users can think in
// terms of their GPU rather than the ffmpeg encoder name.
func findEncoder(short string) (Encoder, bool) {
	switch strings.ToLower(short) {
	case "nvenc", "nvidia":
		return CandidateEncoders()[0], true
	case "amf", "amd":
		return CandidateEncoders()[1], true
	case "qsv", "intel":
		return CandidateEncoders()[2], true
	case "vaapi":
		return CandidateEncoders()[3], true
	case "x264", "software", "soft":
		return SoftwareEncoder(), true
	}
	return Encoder{}, false
}

// argsFor returns reasonable encoder options for a verbatim (not shorthand)
// DAWVC_ENCODER value, keyed on the encoder family name.
func argsFor(name string) []string {
	switch {
	case strings.Contains(name, "nvenc"):
		return []string{"-rc", "vbr", "-cq", "18"}
	case strings.Contains(name, "amf"):
		return []string{"-quality", "balanced", "-rc-mode", "cqp", "-quantizer", "18"}
	case strings.Contains(name, "qsv"):
		return []string{"-preset", "mediumspeed", "-global_quality", "18"}
	case strings.Contains(name, "vaapi"):
		return []string{"-qp", "18"}
	default:
		return []string{"-preset", "veryfast", "-crf", "18"}
	}
}

// probeEncoder runs a 1-frame encode from a lavfi test source through the
// candidate encoder. Exit status 0 means the encoder + hardware + driver are
// all available on this machine right now.
func probeEncoder(ffmpegPath string, enc Encoder) bool {
	if ffmpegPath == "" {
		return false
	}
	args := []string{
		"-hide_banner", "-loglevel", "error", "-y",
		"-f", "lavfi", "-i", "testsrc2=duration=0.1:size=64x64:rate=10",
		"-frames:v", "1",
		"-c:v", enc.Name,
	}
	args = append(args, enc.Args...)
	args = append(args, "-f", "null", "-")
	cmd := exec.Command(ffmpegPath, args...)
	if _, err := cmd.CombinedOutput(); err != nil {
		return false
	}
	return true
}
