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
