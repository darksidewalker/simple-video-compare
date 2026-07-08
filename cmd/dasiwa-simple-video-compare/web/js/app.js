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

  wireDropZone(document.getElementById('dropA'), document.getElementById('pathA'), videoA);
  wireDropZone(document.getElementById('dropB'), document.getElementById('pathB'), videoB);
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

  document.getElementById('statusBtn').addEventListener('click', openRuntimeDialog);
  document.getElementById('closeRuntime').addEventListener('click', () => {
    document.getElementById('runtimeDialog').close();
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
        <div><strong style="color:var(--text)">Root Dir:</strong> <code style="color:var(--amber);font-size:12px">${runtime.root_dir || '—'}</code></div>
      </div>`;
  } catch {
    summary.innerHTML = '<small style="color:var(--red)">Runtime unavailable</small>';
  }
}
