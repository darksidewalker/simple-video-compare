document.addEventListener('DOMContentLoaded', () => {
  const videoA = document.getElementById('videoA');
  const videoB = document.getElementById('videoB');
  const compare = new VideoCompare(videoA, videoB);
  const localBrowser = createLocalBrowser({ rootDir: '' });
  const seek = document.getElementById('seek');
  const playBtn = document.getElementById('playBtn');
  const timeLabel = document.getElementById('timeLabel');
  const viewer = document.getElementById('viewer');
  const sliderHandle = document.getElementById('sliderHandle');
  const blendRange = document.getElementById('blendRange');
  const blendControl = document.getElementById('blendControl');
  const toggleControlsBtn = document.getElementById('toggleControlsBtn');
  const videoDetailsBtn = document.getElementById('videoDetailsBtn');
  const pathA = document.getElementById('pathA');
  const pathB = document.getElementById('pathB');
  const videoDetailsState = window.videoDetailsState = { detailsVisible: false, cachedProbes: {} };

  // Wire drop zones — show details button when at least one video is loaded
  function checkAnyLoaded() {
    const hasA = pathA.value.length > 0;
    const hasB = pathB.value.length > 0;
    if (hasA || hasB) videoDetailsBtn.style.display = '';
    else videoDetailsBtn.style.display = 'none';
    if (hasA && hasB && !videoDetailsState.detailsVisible) loadAllProbes();
  }
  wireDropZone(document.getElementById('dropA'), document.getElementById('pathA'), videoA);
  wireDropZone(document.getElementById('dropB'), document.getElementById('pathB'), videoB);
  pathA.addEventListener('input', checkAnyLoaded);
  pathB.addEventListener('input', checkAnyLoaded);
  document.getElementById('browseA').addEventListener('click', () => localBrowser.open({ videoId: 'videoA', inputId: 'pathA' }));
  document.getElementById('browseB').addEventListener('click', () => localBrowser.open({ videoId: 'videoB', inputId: 'pathB' }));

  playBtn.addEventListener('click', () => {
    playBtn.textContent = compare.playPause() ? 'Pause' : 'Play';
  });

  document.getElementById('syncBtn').addEventListener('click', () => compare.syncToA());
  document.getElementById('audioMode').addEventListener('change', event => compare.setAudio(event.target.value));

  document.querySelectorAll('[data-mode]').forEach(tab => {
    tab.addEventListener('click', () => setCompareMode(tab.dataset.mode));
  });
  blendRange.addEventListener('input', () => {
    viewer.style.setProperty('--blend', String(Number(blendRange.value) / 100));
  });
  sliderHandle.addEventListener('pointerdown', event => {
    event.preventDefault();
    sliderHandle.setPointerCapture(event.pointerId);
    updateSplitFromPointer(event);
  });
  sliderHandle.addEventListener('pointermove', event => {
    if (sliderHandle.hasPointerCapture(event.pointerId)) updateSplitFromPointer(event);
  });
  viewer.addEventListener('pointerdown', event => {
    if (!viewer.classList.contains('slider-mode')) return;
    if (event.target.closest('.viewer-controls')) return;
    updateSplitFromPointer(event);
  });
  toggleControlsBtn.addEventListener('click', () => {
    const hidden = viewer.classList.toggle('controls-hidden');
    toggleControlsBtn.classList.toggle('active', hidden);
    toggleControlsBtn.textContent = hidden ? 'Show controls' : 'Hide controls';
    toggleControlsBtn.title = hidden ? 'Show controls' : 'Hide controls';
  });
  videoDetailsBtn.addEventListener('click', () => {
    if (videoDetailsState.detailsVisible) {
      hideVideoDetails();
    } else {
      showVideoDetails();
    }
  });

  // Status dialog
  document.getElementById('statusBtn').addEventListener('click', openRuntimeDialog);
  document.getElementById('closeRuntime').addEventListener('click', () => {
    document.getElementById('runtimeDialog').close();
  });

  // Quit button
  document.getElementById('quitBtn').addEventListener('click', () => {
    fetch('/api/shutdown', { method: 'POST' })
      .then(r => r.text())
      .then(msg => {
        console.log('Shutting down:', msg);
        setTimeout(() => window.close(), 2000);
      })
      .catch(err => {
        console.error('Shutdown failed:', err);
        alert('Failed to shut down. Please close manually.');
      });
  });

  seek.addEventListener('input', () => {
    compare.isSeeking = true;
    compare.seekRatio(seek.value, seek.max);
    updateTime();
  });
  seek.addEventListener('change', () => { compare.isSeeking = false; });

  videoA.addEventListener('timeupdate', () => {
    if (!compare.isSeeking) updateSeek();
    updateTime();
  });

  videoA.addEventListener('play', () => { if (videoB.paused) videoB.play(); });
  videoA.addEventListener('pause', () => { if (!videoB.paused) videoB.pause(); });
  videoA.addEventListener('seeking', () => compare.syncToA());

  setInterval(() => {
    if (!videoA.paused) compare.syncToA();
  }, 500);

  pingServer();
  setCompareMode('side');
  setInterval(pingServer, 30000);

  // Sync toggle button initial state with actual controls visibility
  const controlsInitiallyHidden = viewer.classList.contains('controls-hidden');
  toggleControlsBtn.textContent = controlsInitiallyHidden ? 'Show controls' : 'Hide controls';
  toggleControlsBtn.title = controlsInitiallyHidden ? 'Show controls' : 'Hide controls';
  toggleControlsBtn.classList.toggle('active', controlsInitiallyHidden);

  function setCompareMode(mode) {
    viewer.classList.remove('side-mode', 'slider-mode', 'blend-mode', 'diff-mode');
    viewer.classList.add(`${mode}-mode`);
    document.querySelectorAll('[data-mode]').forEach(tab => {
      tab.classList.toggle('active', tab.dataset.mode === mode);
    });
    updateModeControls(mode);
    compare.syncToA();
  }

  function updateModeControls(mode) {
    blendControl.hidden = mode !== 'blend';
    viewer.style.setProperty('--blend', String(Number(blendRange.value) / 100));
  }

  function updateSplitFromPointer(event) {
    const rect = viewer.getBoundingClientRect();
    if (!rect.width) return;
    const percent = Math.max(0, Math.min(100, ((event.clientX - rect.left) / rect.width) * 100));
    viewer.style.setProperty('--split', percent.toFixed(2) + '%');
  }

  function updateSeek() {
    const duration = compare.duration();
    if (!duration) return;
    seek.value = String(Math.round((compare.currentTime() / duration) * Number(seek.max)));
  }

  function updateTime() {
    timeLabel.textContent = `${formatTime(compare.currentTime())} / ${formatTime(compare.duration())}`;
  }
});

async function pingServer() {
  const dot = document.getElementById('statusDot');
  try {
    await api.get('/health');
    dot.classList.add('online');
  } catch {
    dot.classList.remove('online');
  }
}

async function openRuntimeDialog() {
  const dialog = document.getElementById('runtimeDialog');
  const summary = document.getElementById('runtimeSummary');
  summary.innerHTML = '<small>Loading…</small>';
  dialog.showModal();
  try {
    const runtime = await api.get('/api/runtime');
    const ffmpeg = runtime.tools && runtime.tools.ready ? 'FFmpeg ready' : 'FFmpeg missing';
    summary.innerHTML = `
      <div style="display:flex;flex-direction:column;gap:6px;color:var(--muted);font-size:13px;">
        <div><strong style="color:var(--text)">UX Mode:</strong> ${runtime.ux_mode || '—'}</div>
        <div><strong style="color:var(--text)">FFmpeg:</strong> ${ffmpeg}</div>
      </div>`;
  } catch {
    summary.innerHTML = '<small style="color:var(--red)">Runtime unavailable</small>';
  }
}

async function loadAllProbes() {
  const pA = document.getElementById('pathA').value.trim();
  const pB = document.getElementById('pathB').value.trim();
  if (!pA || !pB) return;
  try {
    const [rA, rB] = await Promise.all([
      api.post('/api/video/probe', { path: pA }),
      api.post('/api/video/probe', { path: pB }),
    ]);
    const state = window.videoDetailsState;
    if (rA && !rA.error) state.cachedProbes.A = rA;
    if (rB && !rB.error) state.cachedProbes.B = rB;
    if (state.detailsVisible) renderAllDetails();
  } catch (e) {
    console.warn('Video probe failed:', e);
  }
}

function formatDuration(sec) {
  if (!sec || sec <= 0) return '—';
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = Math.round(sec % 60);
  const parts = [];
  if (h > 0) parts.push(h + 'h');
  if (m > 0) parts.push(m + 'm');
  parts.push(s + 's');
  return parts.join(' ');
}

function formatBitrate(bps) {
  if (!bps || bps <= 0) return '—';
  if (bps >= 1e6) return (bps / 1e6).toFixed(2) + ' Mbps';
  if (bps >= 1e3) return (bps / 1e3).toFixed(0) + ' Kbps';
  return bps + ' bps';
}

function formatSize(bytes) {
  if (!bytes || bytes <= 0) return '—';
  if (bytes >= 1e9) return (bytes / 1e9).toFixed(2) + ' GB';
  if (bytes >= 1e6) return (bytes / 1e6).toFixed(1) + ' MB';
  if (bytes >= 1e3) return (bytes / 1e3).toFixed(0) + ' KB';
  return bytes + ' B';
}

function parseFPS(str) {
  if (!str) return null;
  if (str.includes('/')) {
    const [n, d] = str.split('/').map(Number);
    if (d && n) return (n / d).toFixed(3);
  }
  const v = parseFloat(str);
  return isNaN(v) ? null : v.toFixed(3);
}

function streamLabel(s) {
  if (s.codecType === 'video') return 'Video Stream #' + s.index;
  if (s.codecType === 'audio') return 'Audio Stream #' + s.index;
  return 'Stream #' + s.index + ' (' + (s.codecType || 'unknown') + ')';
}

function renderDetailPanel(key, data) {
  const container = document.getElementById('detailPanel' + key);
  if (!container || !data) {
    container.innerHTML = '<div class="detail-section"><span style="color:var(--muted)">No data</span></div>';
    return;
  }
  const f = data.format || {};
  const streams = data.streams || [];
  const videoStreams = streams.filter(s => s.codecType === 'video');
  const audioStreams = streams.filter(s => s.codecType === 'audio');

  let html = '';

  // Header
  html += `<div class="detail-header">`;
  html += `<span class="badge">${key}</span>`;
  html += `<strong>${f.filename || 'Unknown'}</strong>`;
  html += `</div>`;

  // Format section
  html += `<div class="detail-section"><div class="detail-section-title">Format</div>`;
  html += detailRow('Duration', formatDuration(f.duration));
  html += detailRow('File size', formatSize(f.size));
  html += detailRow('Bit rate', formatBitrate(f.bitRate));
  html += `</div>`;

  // Video streams
  if (videoStreams.length > 0) {
    videoStreams.forEach(vs => {
      html += `<div class="detail-section">`;
      html += `<div class="detail-stream-type video">Video</div>`;
      html += detailRow('Codec', vs.codecName || '—');
      html += detailRow('Profile', vs.profile || '—');
      html += detailRow('Resolution', vs.width && vs.height ? vs.width + '×' + vs.height : '—');
      const fps = parseFPS(vs.rFrameRate);
      html += detailRow('Frame rate', fps ? fps + ' fps' : '—');
      html += detailRow('Bit rate', formatBitrate(vs.bitRate));
      html += detailRow('Color space', vs.colorSpace || '—');
      html += detailRow('Color range', vs.colorRange || '—');
      html += detailRow('Primaries', vs.colorPrimaries || '—');
      html += detailRow('Transfer', vs.colorTransfer || '—');
      html += `</div>`;
    });
  }

  // Audio streams
  if (audioStreams.length > 0) {
    audioStreams.forEach(as => {
      html += `<div class="detail-section">`;
      html += `<div class="detail-stream-type audio">Audio</div>`;
      html += detailRow('Codec', as.codecName || '—');
      html += detailRow('Channels', as.channels || '—');
      html += detailRow('Sample rate', as.sampleRate ? as.sampleRate + ' Hz' : '—');
      html += detailRow('Bit rate', formatBitrate(as.bitRate));
      html += `</div>`;
    });
  }

  container.innerHTML = html;
}

function detailRow(label, value) {
  const cls = typeof value === 'string' && /^[a-z0-9]+$/i.test(value.replace(/\s/g, '')) ? ' detail-value code' : ' detail-value';
  return `<div class="detail-row"><span class="detail-label">${label}</span><span class="${cls}">${value || '—'}</span></div>`;
}

function showVideoDetails() {
  const state = window.videoDetailsState;
  const viewer = document.getElementById('viewer');
  const videoDetailsBtn = document.getElementById('videoDetailsBtn');
  state.detailsVisible = true;
  viewer.classList.add('details-visible');
  const overlay = document.getElementById('detailsOverlay');
  overlay.hidden = false;
  videoDetailsBtn.textContent = '✕ Close';
  videoDetailsBtn.title = 'Close video details';
  renderAllDetails();
}

function hideVideoDetails() {
  const state = window.videoDetailsState;
  const viewer = document.getElementById('viewer');
  const videoDetailsBtn = document.getElementById('videoDetailsBtn');
  state.detailsVisible = false;
  viewer.classList.remove('details-visible');
  const overlay = document.getElementById('detailsOverlay');
  overlay.hidden = true;
  videoDetailsBtn.textContent = '📋 Details';
  videoDetailsBtn.title = 'Show video details';
}

function renderAllDetails() {
  const cachedProbes = window.videoDetailsState.cachedProbes;
  if (cachedProbes.A) renderDetailPanel('A', cachedProbes.A);
  if (cachedProbes.B) renderDetailPanel('B', cachedProbes.B);
}