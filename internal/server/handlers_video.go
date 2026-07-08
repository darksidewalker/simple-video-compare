package server

import (
	"encoding/json"
	"fmt"
	"net/http"
	"os/exec"
)

type ProbeRequest struct {
	Path string `json:"path"`
}

type ProbeResponse struct {
	Format   ProbeFormat   `json:"format"`
	Streams  []ProbeStream `json:"streams"`
}

type ProbeFormat struct {
	Filename string `json:"filename"`
	Duration float64 `json:"duration"`
	Size     int64  `json:"size"`
	BitRate  int64  `json:"bit_rate"`
}

type ProbeStream struct {
	Index       int    `json:"index"`
	CodecType   string `json:"codec_type"`
	CodecName   string `json:"codec_name"`
	Width       int    `json:"width"`
	Height      int    `json:"height"`
	RFrameRate  string `json:"r_frame_rate"`
	BitRate     int64  `json:"bit_rate"`
	SampleRate  int    `json:"sample_rate"`
	Channels    int    `json:"channels"`
	Profile     string `json:"profile"`
	ColorSpace  string `json:"color_space"`
	ColorRange  string `json:"color_range"`
	ColorPrimaries string `json:"color_primaries"`
	ColorTransfer string `json:"color_transfer"`
}

func (s *Server) handleVideoProbe(w http.ResponseWriter, r *http.Request) {
	if r.Method != "POST" {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var req ProbeRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]string{"error": "Invalid request body"})
		return
	}

	if req.Path == "" {
		writeJSON(w, http.StatusBadRequest, map[string]string{"error": "Missing video path"})
		return
	}

	// Execute ffprobe command
	cmd := exec.Command("ffprobe", "-v", "quiet", "-print_format", "json", "-show_format", "-show_streams", req.Path)
	output, err := cmd.Output()
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]string{"error": fmt.Sprintf("ffprobe error: %v", err)})
		return
	}

	// Parse ffprobe JSON output
	var rawOutput map[string]interface{}
	if err := json.Unmarshal(output, &rawOutput); err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]string{"error": "Failed to parse ffprobe output"})
		return
	}

	// Extract format information
	format := ProbeFormat{}
	if f, ok := rawOutput["format"].(map[string]interface{}); ok {
		if filename, ok := f["filename"].(string); ok {
			format.Filename = filename
		}
		if duration, ok := f["duration"].(string); ok {
			fmt.Sscanf(duration, "%f", &format.Duration)
		}
		if size, ok := f["size"].(string); ok {
			fmt.Sscanf(size, "%d", &format.Size)
		}
		if bitRate, ok := f["bit_rate"].(string); ok {
			fmt.Sscanf(bitRate, "%d", &format.BitRate)
		}
	}

	// Extract stream information
	streams := []ProbeStream{}
	if streamsRaw, ok := rawOutput["streams"].([]interface{}); ok {
		for _, s := range streamsRaw {
			if stream, ok := s.(map[string]interface{}); ok {
				ps := ProbeStream{}
				if index, ok := stream["index"].(float64); ok {
					ps.Index = int(index)
				}
				if codecType, ok := stream["codec_type"].(string); ok {
					ps.CodecType = codecType
				}
				if codecName, ok := stream["codec_name"].(string); ok {
					ps.CodecName = codecName
				}
				if width, ok := stream["width"].(float64); ok {
					ps.Width = int(width)
				}
				if height, ok := stream["height"].(float64); ok {
					ps.Height = int(height)
				}
				if rFrameRate, ok := stream["r_frame_rate"].(string); ok {
					ps.RFrameRate = rFrameRate
				}
				if bitRate, ok := stream["bit_rate"].(string); ok {
					fmt.Sscanf(bitRate, "%d", &ps.BitRate)
				}
				if sampleRate, ok := stream["sample_rate"].(string); ok {
					fmt.Sscanf(sampleRate, "%d", &ps.SampleRate)
				}
				if channels, ok := stream["channels"].(float64); ok {
					ps.Channels = int(channels)
				}
				if profile, ok := stream["profile"].(string); ok {
					ps.Profile = profile
				}
				if colorSpace, ok := stream["color_space"].(string); ok {
					ps.ColorSpace = colorSpace
				}
				if colorRange, ok := stream["color_range"].(string); ok {
					ps.ColorRange = colorRange
				}
				if colorPrimaries, ok := stream["color_primaries"].(string); ok {
					ps.ColorPrimaries = colorPrimaries
				}
				if colorTransfer, ok := stream["color_transfer"].(string); ok {
					ps.ColorTransfer = colorTransfer
				}
				streams = append(streams, ps)
			}
		}
	}

	response := ProbeResponse{
		Format:  format,
		Streams: streams,
	}

	writeJSON(w, http.StatusOK, response)
}
