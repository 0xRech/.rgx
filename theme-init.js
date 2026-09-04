(() => {
  const root = document.documentElement;
  let selected = 'system';
  try {
    const stored = localStorage.getItem('rgx-theme');
    if (stored === 'light' || stored === 'dark' || stored === 'system') selected = stored;
  } catch {}
  const systemLight = window.matchMedia && window.matchMedia('(prefers-color-scheme: light)').matches;
  root.dataset.themeMode = selected;
  root.dataset.theme = selected === 'system' ? (systemLight ? 'light' : 'dark') : selected;
})();
