# DaSiWa Simple Video Compare

![Preview](assets/preview.png)

Ein kleines Go-Single-Binary-Tool zum seitlichen Vergleich zweier Videodateien mit eingebetteter Dark-Cyber-Lokal-UX im Browser.

## Features

- **Vergleichsmodi**: Side-by-Side, Slider, Blend und Difference
- **Lokale Datei-Auswahl**: Dateibrowser öffnet sich direkt auf dem Server-Rechner per absolutem Pfad
- **Token-basierte Medien-Zustellung**: Ausgewählte Videos werden sicher über `/media/{token}/...` bereitgestellt
- **FFmpeg/FFprobe-Erkennung**: Werden automatisch aus PATH detektiert und im Runtime-Panel angezeigt
- **Native Wiedergabe**: Abhängig von den Codec-Fähigkeiten des eingebetteten Browsers; FFmpeg-Proxy/Cache als nächste Fallback-Schicht
- **RAM-basierter Cache**: Vorbereitende Pufferung für flüssiges Suchen und Abspielen
- **Konfigurierbar**: Host und Port lassen sich über Flags setzen (Standard: `127.0.0.1:8765`)

## Voraussetzungen

- Go 1.22 oder neuer
- FFmpeg und FFprobe (müssen im PATH verfügbar sein)
- Chromium/Chrome-kompatibler Browser (optional, für App-Window-Modus)

## Schnellstart

```bash
go run ./cmd/dasiwa-simple-video-compare
```

Ohne Browserfenster öffnen:

```bash
go run ./cmd/dasiwa-simple-video-compare --no-open
```

Normaler Browser-Tab statt App-Window:

```bash
go run ./cmd/dasiwa-simple-video-compare --browser
```

Mit benutzerdefiniertem Host und Port:

```bash
go run ./cmd/dasiwa-simple-video-compare --host 0.0.0.0 --port 9000
```

## Build

```bash
go build -o ./dist/dasiwa-simple-video-compare ./cmd/dasiwa-simple-video-compare
```

Das kompilierte Binary liegt dann unter `dist/dasiwa-simple-video-compare`.

## Projektstruktur

```
├── cmd/dasiwa-simple-video-compare/
│   ├── main.go              # Einstiegspunkt, CLI-Flags, embed
│   └── web/                 # Eingebettete Frontend-Assets (HTML/CSS/JS)
├── internal/
│   ├── app/                 # Browser-App-Window-Launcher
│   ├── media/               # FFmpeg/FFprobe-Werkzeugerkennung
│   └── server/              # HTTP-Server, Routes, Handler, Cache
├── assets/                  # Projekt-Assets (Screenshots, Icons)
│   ├── preview.png          # Vorschaubild der Oberfläche
│   └── preview.svg          # SVG-Quelle des Vorschaubildes
├── dist/                    # Kompilierte Binaries
└── go.mod                   # Go-Moduldefinition
```

## API-Endpunkte

| Methode | Pfad                | Beschreibung                     |
|---------|---------------------|----------------------------------|
| GET     | /health             | Healthcheck                      |
| GET     | /api/runtime        | Runtime-Info (Tools, Version)    |
| GET     | /api/browse         | Lokalen Dateibaum durchsuchen    |
| POST    | /api/media/register | Video registrieren               |
| POST    | /api/media/cache    | Video in RAM cachen              |
| GET     | /media/*            | Token-basierten Videozugriff     |

## Lizenz

Eigenentwicklung im Rahmen von DaSiWa Tooling.
