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

async function loadDroppedVideo(file, input, video) {
  const oldUrl = video.dataset.objectUrl;
  if (oldUrl) URL.revokeObjectURL(oldUrl);
  const localPath = file.path || file.webkitRelativePath || '';
  input.value = localPath || file.name;
  input.dispatchEvent(new Event('input', { bubbles: true }));
  if (localPath) {
    try {
      const media = await api.post('/api/media/register', { path: localPath });
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
  video.src = url;
  video.preload = 'auto';
  video.dataset.objectUrl = url;
  video.load();
}
