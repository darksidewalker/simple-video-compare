// Tauri drag & drop: the OS delivers drops to the Rust shell (Rust-side
// WindowEvent::DragDrop), which calls this with the cursor position (CSS
// pixels) and, on drop, the absolute file paths. We route to the drop card
// actually under the cursor via getBoundingClientRect — more robust than a
// fixed split — and load through the normal loadDroppedVideo flow, which
// registers the file with the Go server (H.264 proxy + Details probes).
function dawvcCardAt(x, y) {
  const cards = [document.getElementById('dropA'), document.getElementById('dropB')];
  for (const card of cards) {
    if (!card) continue;
    const rect = card.getBoundingClientRect();
    if (x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom) return card;
  }
  return null;
}

window.__dawvcDragDrop = (action, x, y, paths) => {
  const setDragging = (side, active) => {
    const card = document.getElementById('drop' + side);
    if (card) card.classList.toggle('dragging', active);
  };
  if (action === 'enter') {
    const hit = dawvcCardAt(x, y);
    const side = hit && hit.id === 'dropB' ? 'B' : 'A';
    setDragging(side, true);
    return;
  }
  if (action === 'leave') {
    setDragging('A', false);
    setDragging('B', false);
    return;
  }
  if (action !== 'drop' || !Array.isArray(paths) || !paths.length) return;
  const hit = dawvcCardAt(x, y);
  const side = hit && hit.id === 'dropB' ? 'B' : 'A';
  const otherSide = side === 'A' ? 'B' : 'A';
  const load = (path, sideName) => {
    const input = document.getElementById('path' + sideName);
    const video = document.getElementById('video' + sideName);
    if (!input || !video) return;
    loadDroppedVideo({ path, name: path.split('/').pop() }, input, video);
  };
  load(paths[0], side);
  if (paths.length > 1) load(paths[1], otherSide);
  setDragging('A', false);
  setDragging('B', false);
};

function wireDropZone(card, input, video, otherTarget) {
  const setHover = active => card.classList.toggle('dragging', active);

  card.addEventListener('dragover', event => {
    event.preventDefault();
    setHover(true);
  });

  card.addEventListener('dragleave', () => setHover(false));

  card.addEventListener('drop', event => {
    event.preventDefault();
    setHover(false);
    // Inside the native Tauri window, drops are delivered by the OS through the
    // `tauri://drag-drop` event (handled via window.handleTauriDropEvent) with
    // the real absolute path. Ignore the browser-level drop so we don't load
    // the same file twice — the browser File object carries no usable path.
    if (window.__TAURI__) return;
    const files = Array.from(event.dataTransfer.files || []);
    if (!files.length) return;
    loadDroppedVideo(files[0], input, video);
    if (files.length > 1 && otherTarget) {
      loadDroppedVideo(files[1], otherTarget.input, otherTarget.video);
    }
  });
}

// Loading spinner for a video panel: pass state 'busy' (optionally with a
// label) to show it, 'error' to show the failure hint, or null to hide it.
// Used by dropzone.js and filebrowser.js while media is registered,
// transcoded on the server, or fetched into the <video> element.
function setMediaLoading(video, state, label) {
  const panel = video.closest('.video-panel');
  const el = panel && panel.querySelector('.media-loading');
  if (!el) return;
  const labelEl = el.querySelector('.loading-label');
  if (state) {
    el.classList.add('busy');
    el.classList.toggle('error', state === 'error');
    labelEl.textContent = label || '';
  } else {
    el.classList.remove('busy', 'error');
    labelEl.textContent = '';
  }
}

// Show the spinner with a label and keep it up until the <video> element
// reports loadedmetadata (or an error event, which shows the error state).
function armSpinnerOnLoaded(video, label) {
  setMediaLoading(video, 'busy', label || 'Loading…');
  video.addEventListener('loadedmetadata', () => setMediaLoading(video, null), { once: true });
  video.addEventListener('error', () => setMediaLoading(video, 'error', 'Failed to load'), { once: true });
}

async function loadDroppedVideo(file, input, video) {
  const oldUrl = video.dataset.objectUrl;
  if (oldUrl) URL.revokeObjectURL(oldUrl);
  const localPath = file.path || file.webkitRelativePath || '';
  // The input always holds the source file the user picked: Details probes
  // that, and the topbar shows a real filename instead of a /tmp proxy path.
  input.value = localPath || file.name;
  input.dataset.viaProxy = '';
  input.dispatchEvent(new Event('input', { bubbles: true }));
  if (localPath) {
    setMediaLoading(video, 'busy', 'Preparing…');
    try {
      const media = await api.registerMedia(localPath);
      markProxySource(input, media.path, localPath);
      armSpinnerOnLoaded(video, 'Loading…');
      video.src = media.url;
      video.preload = 'auto';
      video.removeAttribute('data-object-url');
      video.load();
      return;
    } catch (error) {
      console.warn('Local media registration failed, falling back to browser blob:', error);
    }
  }
  const url = URL.createObjectURL(file);
  armSpinnerOnLoaded(video, 'Loading…');
  video.src = url;
  video.preload = 'auto';
  video.dataset.objectUrl = url;
  video.load();
}

// Note in the Details panel that playback runs through a server-side H.264
// proxy, i.e. the <video> element plays a different file than the one shown
// in the input. Without this the Details panel would silently describe the
// source while the viewer shows the transcode.
function markProxySource(input, mediaPath, sourcePath) {
  input.dataset.viaProxy = mediaPath && mediaPath !== sourcePath ? 'H.264 proxy' : '';
  const state = window.videoDetailsState;
  if (state && state.detailsVisible) renderAllDetails();
}
