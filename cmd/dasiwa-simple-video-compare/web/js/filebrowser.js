function createLocalBrowser(options) {
  let currentPath = '';
  let target = null;

  const dialog = document.getElementById('fileBrowserDialog');
  const pathBar = document.getElementById('fileBrowserPath');
  const list = document.getElementById('fileBrowserList');
  const closeBtn = document.getElementById('fileBrowserClose');
  const upBtn = document.getElementById('fileBrowserUp');

  closeBtn.addEventListener('click', () => dialog.close());
  upBtn.addEventListener('click', () => browse(filepathParent(currentPath)));

  async function open(slot) {
    target = slot;
    await browse(currentPath || options.rootDir || '');
    dialog.showModal();
  }

  async function browse(path) {
    const data = await api.get('/api/browse?path=' + encodeURIComponent(path || ''));
    currentPath = data.path;
    pathBar.textContent = data.path;
    render(data.items || []);
  }

  function render(items) {
    list.innerHTML = '';
    for (const item of items) {
      const row = document.createElement('button');
      row.type = 'button';
      row.className = 'file-row';
      row.textContent = (item.is_dir ? '📁 ' : '🎬 ') + item.name;
      row.addEventListener('click', () => {
        if (item.is_dir) {
          browse(item.path);
          return;
        }
        selectFile(item);
      });
      list.appendChild(row);
    }
  }

  async function selectFile(item) {
    const media = await api.post('/api/media/register', { path: item.path });
    const video = document.getElementById(target.videoId);
    const input = document.getElementById(target.inputId);
    input.value = media.path;
    input.dispatchEvent(new Event('input', { bubbles: true }));
    video.src = media.url;
    video.preload = 'auto';
    video.removeAttribute('data-object-url');
    video.load();
    dialog.close();
  }

  function formatBytes(bytes) {
    const value = Number(bytes || 0);
    if (value < 1024) return value + ' B';
    if (value < 1024 * 1024) return (value / 1024).toFixed(1) + ' KiB';
    if (value < 1024 * 1024 * 1024) return (value / 1024 / 1024).toFixed(1) + ' MiB';
    return (value / 1024 / 1024 / 1024).toFixed(2) + ' GiB';
  }

  function filepathParent(path) {
    const trimmed = String(path || '').replace(/\/+$/, '');
    const idx = trimmed.lastIndexOf('/');
    if (idx <= 0) return '/';
    return trimmed.slice(0, idx);
  }

  return { open };
}
