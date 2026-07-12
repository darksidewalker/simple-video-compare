package player

import (
	"fmt"
	"image"
	"sync"

	mpv "github.com/gen2brain/go-mpv"
)

const (
	frameWidth  = 960
	frameHeight = 540
)

type Player struct {
	mu     sync.Mutex
	mpv    *mpv.Mpv
	render *mpv.RenderContext
	image  *image.RGBA
}

func New(enableAudio bool) (*Player, error) {
	m := mpv.New()
	if m == nil {
		return nil, fmt.Errorf("libmpv could not be initialized")
	}
	for name, value := range map[string]string{
		"vo":                     "libmpv",
		"hwdec":                  "auto-safe",
		"keep-open":              "yes",
		"osc":                    "no",
		"input-default-bindings": "no",
		"pause":                  "yes",
	} {
		if err := m.SetOptionString(name, value); err != nil {
			m.TerminateDestroy()
			return nil, fmt.Errorf("set libmpv option %s: %w", name, err)
		}
	}
	if !enableAudio {
		if err := m.SetOptionString("ao", "null"); err != nil {
			m.TerminateDestroy()
			return nil, fmt.Errorf("disable secondary audio: %w", err)
		}
	}
	if err := m.Initialize(); err != nil {
		m.TerminateDestroy()
		return nil, fmt.Errorf("initialize libmpv: %w", err)
	}
	render, err := m.NewRenderContextSW()
	if err != nil {
		m.TerminateDestroy()
		return nil, fmt.Errorf("create libmpv render context: %w", err)
	}
	return &Player{mpv: m, render: render, image: image.NewRGBA(image.Rect(0, 0, frameWidth, frameHeight))}, nil
}

func (p *Player) Close() {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.render != nil {
		p.render.Free()
		p.render = nil
	}
	if p.mpv != nil {
		p.mpv.TerminateDestroy()
		p.mpv = nil
	}
}

func (p *Player) Load(path string) error {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.mpv.Command([]string{"loadfile", path, "replace"})
}

func (p *Player) TogglePause() (bool, error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	paused, err := p.mpv.GetProperty("pause", mpv.FormatFlag)
	if err != nil {
		return false, err
	}
	next := !paused.(bool)
	return !next, p.mpv.SetProperty("pause", mpv.FormatFlag, next)
}

func (p *Player) SetPaused(paused bool) error {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.mpv.SetProperty("pause", mpv.FormatFlag, paused)
}

func (p *Player) Seek(seconds float64) error {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.mpv.SetProperty("time-pos", mpv.FormatDouble, seconds)
}

func (p *Player) Position() float64 {
	return p.propertyFloat("time-pos")
}

func (p *Player) Duration() float64 {
	return p.propertyFloat("duration")
}

func (p *Player) propertyFloat(name string) float64 {
	p.mu.Lock()
	defer p.mu.Unlock()
	value, err := p.mpv.GetProperty(name, mpv.FormatDouble)
	if err != nil {
		return 0
	}
	result, _ := value.(float64)
	return result
}

func (p *Player) Render() *image.RGBA {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.renderFrame()
	return p.image
}

func (p *Player) RenderUpdated() (*image.RGBA, bool) {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.render == nil || p.render.Update()&mpv.RenderUpdateFrame == 0 {
		return p.image, false
	}
	return p.image, p.renderFrame()
}

func (p *Player) renderFrame() bool {
	if p.render == nil || p.render.RenderSW(frameWidth, frameHeight, p.image.Stride, "rgb0", p.image.Pix) != nil {
		return false
	}
	for offset := 3; offset < len(p.image.Pix); offset += 4 {
		p.image.Pix[offset] = 0xff
	}
	return true
}
