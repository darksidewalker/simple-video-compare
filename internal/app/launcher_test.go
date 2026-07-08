package app

import "testing"

func TestAppWindowCommandUsesChromiumAppMode(t *testing.T) {
	cmd, args, ok := appWindowCommand("http://127.0.0.1:8765", func(name string) (string, bool) {
		if name == "chromium" {
			return "/usr/bin/chromium", true
		}
		return "", false
	})

	if !ok {
		t.Fatal("expected chromium app window command")
	}
	if cmd != "/usr/bin/chromium" {
		t.Fatalf("cmd = %q", cmd)
	}
	assertArg(t, args, "--app=http://127.0.0.1:8765")
	assertArg(t, args, "--new-window")
}

func assertArg(t *testing.T, args []string, want string) {
	t.Helper()
	for _, got := range args {
		if got == want {
			return
		}
	}
	t.Fatalf("missing arg %q in %v", want, args)
}
