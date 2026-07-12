package compare

import "image"

type Mode string

const (
	Side       Mode = "Side"
	Slider     Mode = "Slider"
	Blend      Mode = "Blend"
	Difference Mode = "Difference"
)

func Compose(a, b *image.RGBA, mode Mode, split, blend float64) *image.RGBA {
	bounds := a.Bounds()
	output := image.NewRGBA(bounds)
	if mode == Side {
		return output
	}
	if split < 0 {
		split = 0
	}
	if split > 1 {
		split = 1
	}
	if blend < 0 {
		blend = 0
	}
	if blend > 1 {
		blend = 1
	}
	for y := bounds.Min.Y; y < bounds.Max.Y; y++ {
		for x := bounds.Min.X; x < bounds.Max.X; x++ {
			offset := output.PixOffset(x, y)
			if mode == Slider && float64(x-bounds.Min.X)/float64(bounds.Dx()) < split {
				copy(output.Pix[offset:offset+4], a.Pix[a.PixOffset(x, y):a.PixOffset(x, y)+4])
				continue
			}
			if mode == Slider {
				copy(output.Pix[offset:offset+4], b.Pix[b.PixOffset(x, y):b.PixOffset(x, y)+4])
				continue
			}
			aOffset := a.PixOffset(x, y)
			bOffset := b.PixOffset(x, y)
			for channel := 0; channel < 3; channel++ {
				if mode == Difference {
					output.Pix[offset+channel] = difference(a.Pix[aOffset+channel], b.Pix[bOffset+channel])
				} else {
					output.Pix[offset+channel] = uint8(float64(a.Pix[aOffset+channel])*(1-blend) + float64(b.Pix[bOffset+channel])*blend)
				}
			}
			output.Pix[offset+3] = 0xff
		}
	}
	return output
}

func difference(a, b uint8) uint8 {
	if a > b {
		return a - b
	}
	return b - a
}
