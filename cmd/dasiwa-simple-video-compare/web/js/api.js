// detectNativeVideoCodecs asks the browser which codecs its <video> element
// can decode natively, expressed as ffprobe codec names so the server's
// compatibility gate can compare them against the probed source codec.
function detectNativeVideoCodecs() {
  const video = document.createElement('video');
  const checks = [
    ['video/mp4; codecs="hevc"', 'hevc'],
    ['video/webm; codecs="vp9"', 'vp9'],
    ['video/webm; codecs="av1"', 'av1'],
  ];
  const caps = [];
  for (const [mimeType, codec] of checks) {
    let support = '';
    try {
      support = video.canPlayType(mimeType) || '';
    } catch {
      support = '';
    }
    if (support !== '') caps.push(codec);
  }
  return caps;
}

const NATIVE_VIDEO_CODECS = detectNativeVideoCodecs();

const api = {
  async get(path) {
    const response = await fetch(path, { cache: 'no-store' });
    if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
    return response.json();
  },
  async post(path, body) {
    const response = await fetch(path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!response.ok) throw new Error(await response.text());
    return response.json();
  },
  // Register a local media file for playback. The client's native codec
  // capabilities are sent along so the server can serve the source file
  // directly (no transcode) when the browser can decode it itself.
  async registerMedia(path) {
    return this.post('/api/media/register', { path, capabilities: NATIVE_VIDEO_CODECS });
  },
};
