(() => {
  /* Replace only the favicon. Branding/layout are handled purely by CSS/HTML. */
  document.querySelectorAll('link[rel~="icon"],link[rel="shortcut icon"]').forEach(link => link.remove());
  const icon = document.createElement('link');
  icon.rel = 'icon';
  icon.type = 'image/png';
  icon.sizes = '128x128';
  icon.href = '/assets/favicon-rgx-v5.png?v=20260904-6';
  document.head.appendChild(icon);

  const base = document.createElement('script');
  base.src = '/script-base-v6.js?v=20260904-6';
  base.defer = true;
  document.head.appendChild(base);
})();
