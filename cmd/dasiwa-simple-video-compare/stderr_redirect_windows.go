//go:build windows

package main

// suppressStderr is a no-op on Windows: this binary does not link libmpv, so
// there is no GTK startup noise to suppress, and the platform does not wire
// its null device into the stdio file-descriptor model used by the Linux
// implementation.
func suppressStderr() func() {
	return func() {}
}
