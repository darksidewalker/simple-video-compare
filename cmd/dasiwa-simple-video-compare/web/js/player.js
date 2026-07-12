/**
 * Canvas-based video player using FFmpeg-decoded frames from the server.
 * Every video codec works because ffmpeg handles decoding — no browser codec limits.
 * Renders at up to 30fps by polling /api/frame/{id}?t= endpoints.
 */
class CanvasPlayer {
  constructor(canvasA, canvasB) {
    this.canvasA = canvasA;
    this.canvasB = canvasB;
    this.ctxA = canvasA.getContext('2d', { willReadFrequently: true });
    this.ctxB = canvasB.getContext('2d', { willReadFrequently: true });
    this.isSeeking = false;
    this.playing = false;
    this.durationA = 0;
    this.durationB = 0;
    this.currentTimeA = 0;
    this.currentTimeB = 0;
    this.frameRequestId = null;
    this.fps = 30;
    this.fpsInterval = 1000 / this.fps;
    this.lastFrameTime = 0;
    this.mediaIdA = '';
    this.mediaIdB = '';
    this.imgA = null;
    this.imgB = null;
    this._metaLoadedA = false;
    this._metaLoadedB = false;
  }

  /** Set media IDs and load initial frames */
  async load(idA, idB) {
    this.mediaIdA = idA;
    this.mediaIdB = idB;
    
    if (idA) {
      await this._loadImage(this.canvasA, this.ctxA, idA, 0);
      this._metaLoadedA = true;
    }
    if (idB) {
      await this._loadImage(this.canvasB, this.ctxB, idB, 0);
      this._metaLoadedB = true;
    }
  }

  async _loadImage(canvas, ctx, mediaId, timeSec) {
    try {
      const resp = await fetch(`/api/frame/${mediaId}?t=${timeSec}`);
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
      const blob = await resp.blob();
      const url = URL.createObjectURL(blob);
      
      return new Promise((resolve) => {
        const img = new Image();
        img.onload = () => {
          canvas.width = img.naturalWidth;
          canvas.height = img.naturalHeight;
          
          // Fit image to canvas maintaining aspect ratio
          const cw = canvas.clientWidth;
          const ch = canvas.clientHeight;
          const scale = Math.min(cw / img.naturalWidth, ch / img.naturalHeight);
          const w = img.naturalWidth * scale;
          const h = img.naturalHeight * scale;
          const x = (cw - w) / 2;
          const y = (ch - h) / 2;
          
          ctx.clearRect(0, 0, canvas.width, canvas.height);
          ctx.drawImage(img, x, y, w, h);
          
          URL.revokeObjectURL(url);
          resolve();
        };
        img.onerror = () => {
          URL.revokeObjectURL(url);
          resolve(); // continue even if image fails
        };
        img.src = url;
      });
    } catch (err) {
      console.error('Failed to load frame:', err);
      return;
    }
  }

  setDuration(dur) {
    this.durationA = dur;
    this.durationB = dur;
  }

  play() {
    if (this.playing) return;
    this.playing = true;
    this.lastFrameTime = performance.now();
    this._tick();
  }

  pause() {
    this.playing = false;
    if (this.frameRequestId) {
      cancelAnimationFrame(this.frameRequestId);
      this.frameRequestId = null;
    }
  }

  togglePause() {
    if (this.playing) {
      this.pause();
      return false;
    } else {
      this.play();
      return true;
    }
  }

  seekTo(seconds) {
    this.currentTimeA = Math.max(0, Math.min(seconds, this.durationA));
    this.currentTimeB = this.currentTimeA;
    this._refreshBothFrames();
  }

  syncToA() {
    this.currentTimeB = this.currentTimeA;
    this._refreshFrameB();
  }

  seekRatio(value, max) {
    const target = this.durationA * (Number(value) / Number(max));
    this.seekTo(target);
  }

  duration() {
    return this.durationA || this.durationB;
  }

  currentTime() {
    return this.currentTimeA;
  }

  setAudio(_mode) {
    // Audio not supported in canvas mode — handled at server side
  }

  _tick() {
    if (!this.playing) return;
    
    const now = performance.now();
    const elapsed = now - this.lastFrameTime;

    if (elapsed >= this.fpsInterval) {
      this.lastFrameTime = now - (elapsed % this.fpsInterval);
      this.currentTimeA += elapsed / 1000;
      this.currentTimeB = this.currentTimeA;

      if (this.currentTimeA >= this.durationA) {
        this.pause();
        this.currentTimeA = 0;
        this.currentTimeB = 0;
      } else {
        this._refreshBothFrames();
      }
    }

    this.frameRequestId = requestAnimationFrame(() => this._tick());
  }

  _refreshBothFrames() {
    this._refreshFrameA();
    this._refreshFrameB();
  }

  _refreshFrameA() {
    if (!this.mediaIdA) return;
    const t = Math.floor(this.currentTimeA * 10) / 10;
    this._loadImage(this.canvasA, this.ctxA, this.mediaIdA, t);
  }

  _refreshFrameB() {
    if (!this.mediaIdB) return;
    const t = Math.floor(this.currentTimeB * 10) / 10;
    this._loadImage(this.canvasB, this.ctxB, this.mediaIdB, t);
  }
}
