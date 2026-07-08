function wireDropZone(card, input, video, _nameEl) {
  const setHover = active => card.classList.toggle('dragging', active);

  card.addEventListener('dragover', event => {
    event.preventDefault();
    setHover(true);
  });

  card.addEventListener('dragleave', () => setHover(false));

  card.addEventListener('drop', event => {
    event.preventDefault();
    setHover(false);
    const file = event.dataTransfer.files[0];
    if (!file) return;
    loadDroppedVideo(file, input, video);
  });
}

function loadDroppedVideo(file, input, video) {
  const oldUrl = video.dataset.objectUrl;
  if (oldUrl) URL.revokeObjectURL(oldUrl);
  const url = URL.createObjectURL(file);
  video.src = url;
  video.preload = 'auto';
  video.dataset.objectUrl = url;
  input.value = file.name;
  input.dispatchEvent(new Event('input', { bubbles: true }));
  video.load();
}
