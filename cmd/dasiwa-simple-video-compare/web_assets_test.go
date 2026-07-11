package main

import (
	"io/fs"
	"strings"
	"testing"
)

func TestEmbeddedUIExposesAllCompareModesAndDenseViewerControls(t *testing.T) {
	assets, err := fs.Sub(webFS, "web")
	if err != nil {
		t.Fatal(err)
	}
	indexBytes, err := fs.ReadFile(assets, "index.html")
	if err != nil {
		t.Fatal(err)
	}
	cssBytes, err := fs.ReadFile(assets, "css/app.css")
	if err != nil {
		t.Fatal(err)
	}
	jsBytes, err := fs.ReadFile(assets, "js/app.js")
	if err != nil {
		t.Fatal(err)
	}
	index := string(indexBytes)
	css := string(cssBytes)
	js := string(jsBytes)

	for _, mode := range []string{`data-mode="side"`, `data-mode="slider"`, `data-mode="blend"`, `data-mode="diff"`} {
		if !strings.Contains(index, mode) {
			t.Fatalf("index.html missing compare mode %s", mode)
		}
	}
	if strings.Contains(index, "disabled>Slider") || strings.Contains(index, "disabled>Blend") || strings.Contains(index, "disabled>Difference") {
		t.Fatal("compare mode buttons must be enabled")
	}
	for _, required := range []string{"blendControl", "setCompareMode", "updateModeControls"} {
		if !strings.Contains(index+js, required) {
			t.Fatalf("UI missing %s", required)
		}
	}
	for _, required := range []string{".viewer.slider-mode", ".viewer.blend-mode", ".viewer.diff-mode", "grid-template-columns: minmax(0, 1fr)", "height: clamp(520px"} {
		if !strings.Contains(css, required) {
			t.Fatalf("CSS missing %s", required)
		}
	}
}

func TestEmbeddedUIUsesHideableOverlayControlsAndDraggableFrameSlider(t *testing.T) {
	assets, err := fs.Sub(webFS, "web")
	if err != nil {
		t.Fatal(err)
	}
	indexBytes, err := fs.ReadFile(assets, "index.html")
	if err != nil {
		t.Fatal(err)
	}
	cssBytes, err := fs.ReadFile(assets, "css/app.css")
	if err != nil {
		t.Fatal(err)
	}
	jsBytes, err := fs.ReadFile(assets, "js/app.js")
	if err != nil {
		t.Fatal(err)
	}
	index := string(indexBytes)
	css := string(cssBytes)
	js := string(jsBytes)

	for _, required := range []string{"viewerControls", "toggleControlsBtn", "sliderHandle"} {
		if !strings.Contains(index, required) {
			t.Fatalf("index.html missing %s", required)
		}
	}
	for _, required := range []string{".viewer-controls", ".viewer.controls-hidden .viewer-controls", ".slider-handle", "rgba(8, 14, 26, 0.58)"} {
		if !strings.Contains(css, required) {
			t.Fatalf("CSS missing %s", required)
		}
	}
	for _, required := range []string{"setPointerCapture", "updateSplitFromPointer", "toggleControlsBtn", "controls-hidden"} {
		if !strings.Contains(js, required) {
			t.Fatalf("JS missing %s", required)
		}
	}
}

func TestHideShowButtonStaysOutsideHiddenOverlay(t *testing.T) {
	assets, err := fs.Sub(webFS, "web")
	if err != nil {
		t.Fatal(err)
	}
	indexBytes, err := fs.ReadFile(assets, "index.html")
	if err != nil {
		t.Fatal(err)
	}
	cssBytes, err := fs.ReadFile(assets, "css/app.css")
	if err != nil {
		t.Fatal(err)
	}
	index := string(indexBytes)
	css := string(cssBytes)

	modeTabs := strings.Index(index, `class="mode-tabs"`)
	toggle := strings.Index(index, `id="toggleControlsBtn"`)
	viewerControls := strings.Index(index, `id="viewerControls"`)
	if modeTabs < 0 || toggle < 0 || viewerControls < 0 {
		t.Fatalf("missing mode tabs, toggle, or viewer controls")
	}
	if !(modeTabs < toggle && toggle < viewerControls) {
		t.Fatalf("hide/show button must live in top mode controls, before hidden overlay")
	}
	if strings.Contains(css, ".viewer.controls-hidden #toggleControlsBtn") {
		t.Fatal("hide/show button must not be hidden with viewer overlay")
	}
	if !strings.Contains(css, ".top-toggle") {
		t.Fatal("CSS missing top-toggle styling")
	}
}

func TestVideoDetailsButtonUpdatesAfterProgrammaticLoad(t *testing.T) {
	assets, err := fs.Sub(webFS, "web")
	if err != nil {
		t.Fatal(err)
	}
	appBytes, err := fs.ReadFile(assets, "js/app.js")
	if err != nil {
		t.Fatal(err)
	}
	dropBytes, err := fs.ReadFile(assets, "js/dropzone.js")
	if err != nil {
		t.Fatal(err)
	}
	browserBytes, err := fs.ReadFile(assets, "js/filebrowser.js")
	if err != nil {
		t.Fatal(err)
	}
	app := string(appBytes)
	drop := string(dropBytes)
	browser := string(browserBytes)

	if !strings.Contains(app, "window.videoDetailsState") {
		t.Fatal("video details state must be available to global detail handlers")
	}
	if strings.Contains(app, "console.log('Click handler called") {
		t.Fatal("video details click handler must not contain debug console logs")
	}
	for name, js := range map[string]string{"dropzone.js": drop, "filebrowser.js": browser} {
		if !strings.Contains(js, "dispatchEvent(new Event('input', { bubbles: true }))") {
			t.Fatalf("%s must dispatch input after setting path programmatically", name)
		}
	}
	if !strings.Contains(drop, "Array.from(event.dataTransfer.files || [])") {
		t.Fatal("dropzone.js must handle multi-file drops")
	}
	if !strings.Contains(app, "{ input: pathB, video: videoB }") || !strings.Contains(app, "{ input: pathA, video: videoA }") {
		t.Fatal("app.js must wire paired drop targets so dropping two videos fills both slots")
	}
	if !strings.Contains(app, "updatePanelAspect") || !strings.Contains(app, "--video-aspect") {
		t.Fatal("app.js must update video panel aspect ratios from loaded metadata")
	}
}
