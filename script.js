(() => {
  const header = document.querySelector('.site-header');
  const progress = document.getElementById('pageProgress');
  const cursorGlow = document.getElementById('cursorGlow');
  const menuButton = document.getElementById('menuButton');
  const navLinks = document.getElementById('navLinks');
  const toast = document.getElementById('toast');
  const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  document.documentElement.dataset.theme = 'dark';
  document.querySelector('meta[name="theme-color"]')?.setAttribute('content', '#08090f');
  document.querySelector('meta[name="color-scheme"]')?.setAttribute('content', 'dark');
  try { localStorage.removeItem('rgx-theme'); } catch {}

  /* Stable RGX branding. Use new asset names so long-lived image caches cannot
     keep an older/cropped logo or favicon alive. */
  const WORDMARK = '/assets/rgx-wordmark-v3.png';
  const FAVICON = '/assets/favicon-rgx-v3.png';

  document.querySelectorAll('link[rel~="icon"], link[rel="shortcut icon"]').forEach(link => link.remove());
  const favicon = document.createElement('link');
  favicon.rel = 'icon';
  favicon.type = 'image/png';
  favicon.sizes = '128x128';
  favicon.href = `${FAVICON}?v=20260904-3`;
  document.head.appendChild(favicon);
  const shortcut = document.createElement('link');
  shortcut.rel = 'shortcut icon';
  shortcut.type = 'image/png';
  shortcut.href = `${FAVICON}?v=20260904-3`;
  document.head.appendChild(shortcut);

  const makeWordmark = (alt = 'RGX') => {
    const img = document.createElement('img');
    img.src = WORDMARK;
    img.alt = alt;
    img.decoding = 'async';
    img.draggable = false;
    img.style.setProperty('display', 'block', 'important');
    img.style.setProperty('width', '100%', 'important');
    img.style.setProperty('height', '100%', 'important');
    img.style.setProperty('max-width', '100%', 'important');
    img.style.setProperty('object-fit', 'contain', 'important');
    img.style.setProperty('object-position', 'center', 'important');
    img.style.setProperty('box-sizing', 'border-box', 'important');
    return img;
  };

  document.querySelectorAll('.brand').forEach(brand => {
    brand.replaceChildren(makeWordmark('RGX'));
    brand.style.setProperty('display', 'flex', 'important');
    brand.style.setProperty('align-items', 'center', 'important');
    brand.style.setProperty('justify-content', 'center', 'important');
    brand.style.setProperty('width', '166px', 'important');
    brand.style.setProperty('height', '58px', 'important');
    brand.style.setProperty('min-width', '166px', 'important');
    brand.style.setProperty('padding', '5px 8px', 'important');
    brand.style.setProperty('overflow', 'visible', 'important');
    brand.style.setProperty('background-color', '#eef2fb', 'important');
    brand.style.setProperty('background-image', 'none', 'important');
    brand.style.setProperty('background', '#eef2fb', 'important');
    brand.style.setProperty('border-radius', '13px', 'important');
  });

  document.querySelectorAll('.official-logo-plate').forEach(plate => {
    plate.replaceChildren(makeWordmark('RGX'));
    plate.style.setProperty('display', 'flex', 'important');
    plate.style.setProperty('align-items', 'center', 'important');
    plate.style.setProperty('justify-content', 'center', 'important');
    plate.style.setProperty('width', 'min(390px, 84%)', 'important');
    plate.style.setProperty('height', '166px', 'important');
    plate.style.setProperty('padding', '12px 16px', 'important');
    plate.style.setProperty('overflow', 'visible', 'important');
    plate.style.setProperty('background-color', '#eef2fb', 'important');
    plate.style.setProperty('background-image', 'none', 'important');
    plate.style.setProperty('background', '#eef2fb', 'important');
    plate.style.setProperty('border-radius', '22px', 'important');
  });

  const safeStore = (key, value) => { try { localStorage.setItem(key, value); } catch {} };
  document.querySelectorAll('[data-lang-choice]').forEach(link => {
    link.addEventListener('click', () => safeStore('rgx-lang', link.dataset.langChoice || ''));
  });

  const onScroll = () => {
    const y = window.scrollY;
    header?.classList.toggle('scrolled', y > 10);
    const max = Math.max(document.documentElement.scrollHeight - window.innerHeight, 1);
    if (progress) progress.style.width = `${Math.min((y / max) * 100, 100)}%`;
  };
  onScroll();
  window.addEventListener('scroll', onScroll, { passive: true });

  if (!reduceMotion && window.matchMedia('(pointer:fine)').matches) {
    document.body.classList.add('pointer-active');
    window.addEventListener('pointermove', event => {
      if (!cursorGlow) return;
      cursorGlow.style.left = `${event.clientX}px`;
      cursorGlow.style.top = `${event.clientY}px`;
    }, { passive: true });
  }

  menuButton?.addEventListener('click', () => {
    const open = menuButton.getAttribute('aria-expanded') === 'true';
    menuButton.setAttribute('aria-expanded', String(!open));
    navLinks?.classList.toggle('open', !open);
  });
  navLinks?.querySelectorAll('a').forEach(link => link.addEventListener('click', () => {
    navLinks.classList.remove('open');
    menuButton?.setAttribute('aria-expanded', 'false');
  }));

  const reveals = [...document.querySelectorAll('.reveal')];
  reveals.forEach(el => {
    const delay = el.dataset.delay;
    if (delay) el.style.setProperty('--delay', `${delay}ms`);
  });
  if ('IntersectionObserver' in window && !reduceMotion) {
    const observer = new IntersectionObserver(entries => {
      entries.forEach(entry => {
        if (!entry.isIntersecting) return;
        entry.target.classList.add('visible');
        observer.unobserve(entry.target);
      });
    }, { threshold: .1, rootMargin: '0px 0px -30px' });
    reveals.forEach(el => observer.observe(el));
  } else {
    reveals.forEach(el => el.classList.add('visible'));
  }

  if (!reduceMotion && window.matchMedia('(hover:hover) and (pointer:fine)').matches) {
    document.querySelectorAll('.tilt-card').forEach(card => {
      card.addEventListener('pointermove', event => {
        const rect = card.getBoundingClientRect();
        const x = (event.clientX - rect.left) / rect.width - .5;
        const y = (event.clientY - rect.top) / rect.height - .5;
        card.style.transform = `perspective(1100px) rotateX(${(-y * 3.5).toFixed(2)}deg) rotateY(${(x * 4.5).toFixed(2)}deg) translateY(-1px)`;
      });
      card.addEventListener('pointerleave', () => { card.style.transform = ''; });
    });
  }

  let toastTimer;
  const showToast = message => {
    if (!toast) return;
    toast.textContent = message || toast.dataset.default || 'Copied';
    toast.classList.add('show');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => toast.classList.remove('show'), 1500);
  };
  document.querySelectorAll('.copy-command').forEach(button => {
    button.addEventListener('click', async () => {
      const value = button.dataset.copy || '';
      try {
        await navigator.clipboard.writeText(value);
      } catch {
        const area = document.createElement('textarea');
        area.value = value;
        area.style.cssText = 'position:fixed;opacity:0';
        document.body.appendChild(area);
        area.select();
        document.execCommand('copy');
        area.remove();
      }
      showToast();
    });
  });

  const tabs = [...document.querySelectorAll('[role="tab"]')];
  const panes = [...document.querySelectorAll('[role="tabpanel"]')];
  const activate = tab => {
    const target = tab.dataset.terminal;
    tabs.forEach(item => {
      const active = item === tab;
      item.classList.toggle('active', active);
      item.setAttribute('aria-selected', String(active));
      item.tabIndex = active ? 0 : -1;
    });
    panes.forEach(pane => {
      const active = pane.dataset.pane === target;
      pane.classList.toggle('active', active);
      pane.hidden = !active;
    });
  };
  tabs.forEach((tab, index) => {
    tab.addEventListener('click', () => activate(tab));
    tab.addEventListener('keydown', event => {
      if (!['ArrowLeft','ArrowRight','Home','End'].includes(event.key)) return;
      event.preventDefault();
      const next = event.key === 'Home' ? 0 :
        event.key === 'End' ? tabs.length - 1 :
        event.key === 'ArrowRight' ? (index + 1) % tabs.length :
        (index - 1 + tabs.length) % tabs.length;
      tabs[next].focus();
      activate(tabs[next]);
    });
  });
  if (tabs.length) activate(tabs.find(tab => tab.classList.contains('active')) || tabs[0]);

  document.querySelectorAll('details.tool-menu').forEach(details => {
    details.addEventListener('toggle', () => {
      if (!details.open) return;
      document.querySelectorAll('details.tool-menu').forEach(other => {
        if (other !== details) other.removeAttribute('open');
      });
    });
  });
  document.addEventListener('click', event => {
    if (!event.target.closest('.tool-menu')) {
      document.querySelectorAll('details.tool-menu').forEach(details => details.removeAttribute('open'));
    }
  });

  const year = document.getElementById('year');
  if (year) year.textContent = new Date().getFullYear();
})();
