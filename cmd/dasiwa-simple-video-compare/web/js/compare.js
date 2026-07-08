class VideoCompare {
  constructor(videoA, videoB) {
    this.a = videoA;
    this.b = videoB;
    this.isSeeking = false;
  }

  playPause() {
    if (this.a.paused) {
      this.syncToA();
      this.a.play();
      this.b.play();
      return true;
    }
    this.a.pause();
    this.b.pause();
    return false;
  }

  syncToA() {
    if (!Number.isFinite(this.a.currentTime)) return;
    if (Math.abs(this.b.currentTime - this.a.currentTime) > 0.08) {
      this.b.currentTime = this.a.currentTime;
    }
  }

  seekRatio(value, max) {
    const duration = this.duration();
    if (!duration) return;
    const target = duration * (Number(value) / Number(max));
    this.a.currentTime = target;
    this.b.currentTime = Math.min(target, this.b.duration || target);
  }

  duration() {
    const durations = [this.a.duration, this.b.duration].filter(Number.isFinite);
    return durations.length ? Math.max(...durations) : 0;
  }

  currentTime() {
    return Number.isFinite(this.a.currentTime) ? this.a.currentTime : 0;
  }

  setAudio(mode) {
    this.a.muted = mode !== 'a';
    this.b.muted = mode !== 'b';
  }
}

function formatTime(seconds) {
  if (!Number.isFinite(seconds)) return '00:00.000';
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  const ms = Math.floor((seconds % 1) * 1000);
  return `${String(mins).padStart(2, '0')}:${String(secs).padStart(2, '0')}.${String(ms).padStart(3, '0')}`;
}
