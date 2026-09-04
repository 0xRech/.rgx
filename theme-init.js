(() => {
  const root = document.documentElement;
  root.dataset.theme = 'dark';
  root.dataset.themeMode = 'dark';
  try { localStorage.removeItem('rgx-theme'); } catch {}
})();
