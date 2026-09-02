package media

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// makeFakeFFmpeg writes an executable shell script that emulates an ffmpeg
// build: it "supports" exactly the encoders named in supported (it greps its
// own args for "-c:v <name>") and exits non-zero for everything else, mirroring
// how a real ffmpeg without that HW backend rejects the encoder.
func makeFakeFFmpeg(t *testing.T, dir string, supported []string) string {
	t.Helper()
	var sb strings.Builder
	sb.WriteString("#!/bin/sh\n")
	sb.WriteString("enc=\"\"\nprev=\"\"\n")
	sb.WriteString("for a in \"$@\"; do\n")
	sb.WriteString("  if [ \"$prev\" = \"-c:v\" ]; then enc=\"$a\"; fi\n")
	sb.WriteString("  prev=\"$a\"\n")
	sb.WriteString("done\n")
	for _, s := range supported {
		sb.WriteString("case \"$enc\" in " + s + ") exit 0;; esac\n")
	}
	sb.WriteString("exit 1\n")
	p := filepath.Join(dir, "ffmpeg")
	if err := os.WriteFile(p, []byte(sb.String()), 0755); err != nil {
		t.Fatal(err)
	}
	return p
}

// The first HW encoder present wins.
func TestDetectEncoderPicksFirstSupported(t *testing.T) {
	dir := t.TempDir()
	ff := makeFakeFFmpeg(t, dir, []string{"h264_nvenc"})
	enc := DetectEncoder(ff)
	if enc.Name != "h264_nvenc" {
		t.Fatalf("expected h264_nvenc, got %s", enc.Name)
	}
}

// When nothing is supported the software encoder is always returned.
func TestDetectEncoderFallsBackToSoftware(t *testing.T) {
	dir := t.TempDir()
	ff := makeFakeFFmpeg(t, dir, nil) // supports nothing
	enc := DetectEncoder(ff)
	if enc.Name != "libx264" {
		t.Fatalf("expected libx264 fallback, got %s", enc.Name)
	}
}

// A lower-priority encoder is chosen when higher ones are absent: the
// nvenc/amf candidates are rejected, qsv is accepted.
func TestDetectEncoderOrdering(t *testing.T) {
	dir := t.TempDir()
	ff := makeFakeFFmpeg(t, dir, []string{"h264_qsv", "libx264"})
	enc := DetectEncoder(ff)
	if enc.Name != "h264_qsv" {
		t.Fatalf("expected h264_qsv, got %s", enc.Name)
	}
}

// DAWVC_ENCODER pins a specific encoder, bypassing auto-detection.
func TestDetectEncoderEnvOverride(t *testing.T) {
	dir := t.TempDir()
	ff := makeFakeFFmpeg(t, dir, nil) // nothing works, but override still applies
	t.Setenv("DAWVC_ENCODER", "x264")
	enc := DetectEncoder(ff)
	if enc.Name != "libx264" {
		t.Fatalf("env override x264 must select libx264, got %s", enc.Name)
	}
}

// The software encoder must always be the last candidate.
func TestCandidateEncodersSoftwareLast(t *testing.T) {
	encs := CandidateEncoders()
	if len(encs) == 0 {
		t.Fatal("CandidateEncoders must return at least one encoder")
	}
	if encs[len(encs)-1].Name != "libx264" {
		t.Fatalf("software encoder must be last, got %s", encs[len(encs)-1].Name)
	}
}
