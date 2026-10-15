# Windows native playback

## Decision

Reuse libmpv in-process. The Windows renderer loads `libmpv-2.dll` and supplies a dedicated child HWND through mpv's `wid` option. The video pane is part of the Tauri desktop window, above WebView2 controls. No external mpv player is launched for desktop playback. Comparison filters, metadata and transport retain the existing backend.

Use a 64-bit shared library build, not just `mpv.exe`. See the [mpv installation page](https://mpv.io/installation/) for Windows build sources. Third-party binaries require their own provenance and dependency review.

## Alternatives considered

| Library | Native embedding | Trade-off |
|---|---|---|
| **libmpv (selected)** | C API and Windows HWND through `wid` | Existing backend and FFmpeg filters can be retained. Codec support depends on the exact build. |
| **libVLC** | `libvlc_media_player_set_hwnd()` | Broad playback support, but replacing the current comparison engine and distributing VLC plugins would add work. |
| **GStreamer** | `GstVideoOverlay`, for example with `d3d11videosink` | Flexible custom pipelines, but a larger plugin/deployment and comparison-pipeline change. |
| **Media Foundation / Media Engine** | HWND output or DirectComposition | OS codec availability varies; HEVC and AV1 may need optional codec packs. Poor fit for consistent arbitrary-source comparison. |

## Licensing

The app is GPL-3.0-or-later. Default mpv is GPL-2.0-or-later, which permits a GPLv3 combined distribution. An LGPL build is possible, but `-Dgpl=false` alone does not prove the resulting library and all dependencies are LGPL. The exact FFmpeg configuration can change the license obligations.

libVLC core is LGPL-2.1; audit its bundled plugins and dependencies separately. GStreamer core is LGPL, but plugin sets and their linked libraries need individual review: “Bad” and “Ugly” are not license classifications. Media Foundation uses installed Windows components; do not redistribute OS DLLs or assume optional codec packs are freely redistributable.

Before distributing a bundled libmpv/FFmpeg runtime:

- Record the exact binary provenance, versions, build options and dependency licenses.
- Include required third-party copyright notices and license texts.
- Provide corresponding source matching the shipped GPL components, including modifications and the scripts needed to build/install them, using a distribution method allowed by the license. An upstream HEAD link alone is insufficient.
- Fulfil applicable LGPL source, replacement/relinking and modification obligations for LGPL components.
- Reject builds containing nonfree or GPL-incompatible dependencies.

Copyright-license compatibility is not codec-patent clearance. Patent requirements can depend on jurisdiction and distribution. This is an engineering license assessment, not legal advice. The repository does not currently bundle or approve a particular Windows DLL build.

## Sources

- [mpv embedding API](https://raw.githubusercontent.com/mpv-player/mpv/master/include/mpv/client.h)
- [mpv manual (`wid`)](https://mpv.io/manual/stable/)
- [mpv copyright and GPL/LGPL build notes](https://raw.githubusercontent.com/mpv-player/mpv/master/Copyright)
- [libVLC](https://www.videolan.org/vlc/libvlc.html) and [Windows HWND API](https://videolan.videolan.me/vlc-3.0/group__libvlc__media__player.html)
- [GStreamer VideoOverlay](https://gstreamer.freedesktop.org/documentation/video/gstvideooverlay.html), [D3D11 sink](https://gstreamer.freedesktop.org/documentation/d3d11/d3d11videosink.html) and [licensing FAQ](https://gstreamer.freedesktop.org/documentation/frequently-asked-questions/licensing.html)
- [Media Engine HWND playback](https://learn.microsoft.com/en-us/windows/win32/medfound/mf-media-engine-playback-hwnd) and [Windows codec support](https://learn.microsoft.com/en-us/windows/apps/develop/media-authoring-processing/supported-codecs)
- [FFmpeg legal notes](https://ffmpeg.org/legal.html), [GPLv3](https://www.gnu.org/licenses/gpl-3.0.html) and [LGPLv2.1](https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html)

A cross-build proves compilation, not Windows playback. Target-platform acceptance still requires actual changing pixels, single A/B and paired inputs, comparison modes, unequal-duration tails, transport/audio, resize/DPI/fullscreen, reopen and clean exit under Windows/WebView2.
