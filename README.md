# DaSiWa Simple Video Compare

![Preview](assets/preview.png)

Ein natives Desktop-Tool zum seitlichen Vergleich zweier lokaler Videodateien. Die Wiedergabe läuft über libmpv/FFmpeg statt über ein Browser-`<video>`-Element und ist daher nicht auf Browser-Codecs beschränkt.

## Features

- Standalone-UX: Eigenes Desktop-Fenster, kein Browser und kein lokaler Webserver im Standardmodus
- libmpv-Wiedergabe: Decoder, Demuxer, Untertitel und Hardwarebeschleunigung aus der lokalen mpv/FFmpeg-Installation
- Synchroner Vergleich: Zwei unabhängige native Player, gemeinsame Wiedergabe, Seek und expliziter Sync
- Lokale Datei-Auswahl: Native Dateidialoge oder Datei-Drop direkt auf die linke/rechte Videohälfte; zwei gleichzeitig gedroppte Dateien belegen A und B
- Web-UI: Die Weboberfläche läuft als eingebetteter lokaler HTTP-Server (Port 1420, `DAWVC_PORT` möglich)
- Browser-Autostart: Bei Start mit grafischer Umgebung öffnet sich die Weboberfläche automatisch im Standardbrowser; `--no-browser` deaktiviert das
- Plattform: Windows, Linux und macOS (amd64) — reine Go-Binaries ohne externe Laufzeitabhängigkeiten (nur ffmpeg/ffprobe)

## Voraussetzungen

- Go 1.25 oder neuer (zum Bauen)
- Linux/macOS/Windows: ffmpeg und ffprobe auf dem System (für Details-Panel und H.264-Proxy)

Alle drei Plattformen werden als reine Go-Binaries gebaut (CGO aus, keine libmpv-Verlinkung):

```bash
./build.sh            # Linux, macOS und Windows in einem Durchgang
```

Die fertigen Binaries liegen im Repository-Wurzelverzeichnis:

| Datei | Plattform | Start |
|---|---|---|
| `dasiwa-simple-video-compare-linux-amd64` | Linux | `./dasiwa-simple-video-compare-linux-amd64` |
| `dasiwa-simple-video-compare-darwin-amd64` | macOS (Intel) | `./dasiwa-simple-video-compare-darwin-amd64` |
| `dasiwa-simple-video-compare-windows-amd64.exe` | Windows | `dasiwa-simple-video-compare-windows-amd64.exe` |

## Schnellstart

Die Web-UI starten (öffnet bei Start automatisch im Standardbrowser):

```bash
go run ./cmd/dasiwa-simple-video-compare
```

Ohne Browserfenster öffnen (z. B. als Daemon):

```bash
go run ./cmd/dasiwa-simple-video-compare --no-browser
```

## Build

```bash
go build -ldflags="-s -w" -o dasiwa-simple-video-compare-linux-amd64 ./cmd/dasiwa-simple-video-compare
```

Die Binaries benötigen zur Laufzeit nur das System und `ffmpeg`/`ffprobe`; die Weboberfläche ist eingebettet.

## Projektstruktur

```
├── cmd/dasiwa-simple-video-compare/
│   ├── main.go              # Einstiegspunkt für die Desktop-UX
│   └── web/                 # Eingebettete Assets für den optionalen Browsermodus
├── internal/
│   ├── player/              # libmpv-basierte native Video-Renderer
│   ├── media/               # FFmpeg/FFprobe-Werkzeugerkennung
│   └── server/              # HTTP-Server für den optionalen Browsermodus
├── assets/                  # Projekt-Assets (Screenshots, Icons)
└── go.mod                   # Go-Moduldefinition
```

Die fertigen Plattform-Binaries liegen direkt im Wurzelverzeichnis.

Der Browsermodus stellt weiterhin die bisherigen lokalen HTTP-Endpunkte bereit.

## Lizenz

Eigenentwicklung im Rahmen von DaSiWa Tooling.
