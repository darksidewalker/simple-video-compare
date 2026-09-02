//go:build !windows

package main

import "os"

// suppressStderr temporarily points os.Stderr at the operating system's null
// device so that startup noise (GTK/GDK protocol errors from libmpv rendering
// backends on headless systems) does not reach the user's terminal. The
// returned function must be called once after the guarded section to restore
// the original stderr.
//
// On Windows this binary does not link libmpv, so the guard is a no-op there;
// see stderr_redirect_windows.go.
func suppressStderr() func() {
	backup := os.Stderr
	devNull, err := os.OpenFile(os.DevNull, os.O_WRONLY, 0)
	if err != nil {
		return func() {}
	}
	os.Stderr = devNull
	return func() {
		os.Stderr = backup
		devNull.Close()
	}
}
