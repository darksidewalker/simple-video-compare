package compare

import (
	"image"
	"image/color"
	"testing"
)

func TestComposeModes(t *testing.T) {
	a := solid(color.RGBA{R: 200, A: 255})
	b := solid(color.RGBA{B: 100, A: 255})

	slider := Compose(a, b, Slider, 0.5, 0)
	if got := slider.RGBAAt(0, 0); got.R != 200 || got.B != 0 {
		t.Fatalf("slider left pixel = %#v", got)
	}
	if got := slider.RGBAAt(3, 0); got.R != 0 || got.B != 100 {
		t.Fatalf("slider right pixel = %#v", got)
	}

	blend := Compose(a, b, Blend, 0, 0.5).RGBAAt(0, 0)
	if blend.R != 100 || blend.B != 50 || blend.A != 255 {
		t.Fatalf("blend pixel = %#v", blend)
	}

	diff := Compose(a, b, Difference, 0, 0).RGBAAt(0, 0)
	if diff.R != 200 || diff.B != 100 || diff.A != 255 {
		t.Fatalf("difference pixel = %#v", diff)
	}
}

func solid(c color.RGBA) *image.RGBA {
	output := image.NewRGBA(image.Rect(0, 0, 4, 1))
	for x := 0; x < 4; x++ {
		output.SetRGBA(x, 0, c)
	}
	return output
}
