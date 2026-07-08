function wireDropZone(card, input, video, nameEl) {
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
    loadDroppedVideo(file, input, video, nameEl);
  });
}

function loadDroppedVideo(file, input, video, nameEl) {
  const oldUrl = video.dataset.objectUrl;
  if (oldUrl) URL.revokeObjectURL(oldUrl);
  const url = URL.createObjectURL(file);
  video.src = url;
  video.preload = 'auto';
  video.dataset.objectUrl = url;
  input.value = file.name;
  nameEl.textContent = file.name;
  video.load();
}
