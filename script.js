(() => {
  const root = document.documentElement;
  const header = document.querySelector('.site-header');
  const progress = document.getElementById('pageProgress');
  const cursorGlow = document.getElementById('cursorGlow');
  const menuButton = document.getElementById('menuButton');
  const navLinks = document.getElementById('navLinks');
  const toast = document.getElementById('toast');
  const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const LOGO = '/assets/rgx-logo.png?v=20260904-7';

  root.dataset.theme = 'dark';
  root.dataset.themeMode = 'dark';
  document.querySelector('meta[name="theme-color"]')?.setAttribute('content', '#08090f');
  document.querySelector('meta[name="color-scheme"]')?.setAttribute('content', 'dark');
  try { localStorage.removeItem('rgx-theme'); } catch {}

  // Neutralize legacy pseudo-element branding only. Actual sizing is set inline below.
  const reset = document.createElement('style');
  reset.textContent = '.brand::before,.official-logo-plate::before{content:none!important;display:none!important}';
  document.head.appendChild(reset);

  // Theme switching was removed from RGX. Keep only the language selector.
  document.querySelectorAll('details.tool-menu').forEach((details) => {
    if (details.querySelector('[data-theme-choice]') || details.querySelector('#themeLabel')) details.remove();
  });

  const important = (el, name, value) => el.style.setProperty(name, value, 'important');

  const installWordmark = (container, width, padding, radius, shadow = true) => {
    container.replaceChildren();
    important(container, 'display', 'inline-flex');
    important(container, 'align-items', 'center');
    important(container, 'justify-content', 'center');
    important(container, 'width', 'auto');
    important(container, 'height', 'auto');
    important(container, 'min-width', '0');
    important(container, 'min-height', '0');
    important(container, 'padding', padding);
    important(container, 'overflow', 'visible');
    important(container, 'border-radius', radius);
    important(container, 'background', 'linear-gradient(145deg,#f6f8fd,#e8edf7)');
    important(container, 'border', '1px solid rgba(255,255,255,.20)');
    important(container, 'box-sizing', 'border-box');
    important(container, 'box-shadow', shadow ? '0 8px 24px rgba(0,0,0,.16)' : 'none');

    const img = document.createElement('img');
    img.src = LOGO;
    img.alt = 'RGX';
    img.decoding = 'async';
    img.className = 'rgx-wordmark';
    important(img, 'display', 'block');
    important(img, 'width', width);
    important(img, 'height', 'auto');
    important(img, 'max-width', 'none');
    important(img, 'max-height', 'none');
    important(img, 'object-fit', 'contain');
    important(img, 'object-position', 'center');
    important(img, 'transform', 'none');
    important(img, 'margin', '0');
    important(img, 'padding', '0');
    container.appendChild(img);
  };

  document.querySelectorAll('header .brand').forEach((brand) => installWordmark(brand, '150px', '6px 8px', '12px'));
  document.querySelectorAll('footer .brand').forEach((brand) => installWordmark(brand, '176px', '7px 9px', '13px', false));
  document.querySelectorAll('.official-logo-plate').forEach((plate) => installWordmark(plate, '350px', '16px 18px', '20px'));

  // Redirect any remaining legacy RGX image to the same local file.
  document.querySelectorAll('img').forEach((img) => {
    const src = img.getAttribute('src') || '';
    if (src.includes('user-attachments') || src.includes('rgx-logo.webp') || src.includes('rgx-mark.webp')) {
      if (!img.closest('.brand') && !img.closest('.official-logo-plate')) img.src = LOGO;
    }
  });

  const safeStorage = {
    set(key, value) { try { localStorage.setItem(key, value); } catch {} }
  };
  document.querySelectorAll('[data-lang-choice]').forEach((link) => {
    link.addEventListener('click', () => safeStorage.set('rgx-lang', link.dataset.langChoice));
  });

  const onScroll = () => {
    const y = window.scrollY;
    header?.classList.toggle('scrolled', y > 12);
    const max = Math.max(document.documentElement.scrollHeight - window.innerHeight, 1);
    if (progress) progress.style.width = `${Math.min((y / max) * 100, 100)}%`;
  };
  onScroll();
  window.addEventListener('scroll', onScroll, { passive: true });

  if (!reduceMotion && window.matchMedia('(pointer:fine)').matches) {
    document.body.classList.add('pointer-active');
    window.addEventListener('pointermove', (event) => {
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
  navLinks?.querySelectorAll('a').forEach((link) => link.addEventListener('click', () => {
    navLinks.classList.remove('open');
    menuButton?.setAttribute('aria-expanded', 'false');
  }));

  const reveals = document.querySelectorAll('.reveal');
  reveals.forEach((element) => {
    const delay = element.getAttribute('data-delay');
    if (delay) element.style.setProperty('--delay', `${delay}ms`);
  });
  if ('IntersectionObserver' in window && !reduceMotion) {
    const observer = new IntersectionObserver((entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          entry.target.classList.add('visible');
          observer.unobserve(entry.target);
        }
      });
    }, { threshold: .12, rootMargin: '0px 0px -35px' });
    reveals.forEach((element) => observer.observe(element));
  } else {
    reveals.forEach((element) => element.classList.add('visible'));
  }

  if (!reduceMotion && window.matchMedia('(hover:hover) and (pointer:fine)').matches) {
    document.querySelectorAll('.tilt-card').forEach((card) => {
      card.addEventListener('pointermove', (event) => {
        const rect = card.getBoundingClientRect();
        const x = (event.clientX - rect.left) / rect.width - .5;
        const y = (event.clientY - rect.top) / rect.height - .5;
        card.style.transform = `perspective(1100px) rotateX(${(-y * 4.2).toFixed(2)}deg) rotateY(${(x * 5.2).toFixed(2)}deg) translateY(-1px)`;
      });
      card.addEventListener('pointerleave', () => { card.style.transform = ''; });
    });
  }

  let toastTimer;
  const showToast = (message) => {
    if (!toast) return;
    toast.textContent = message || toast.dataset.default || 'Copied';
    toast.classList.add('show');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => toast.classList.remove('show'), 1600);
  };
  document.querySelectorAll('.copy-command').forEach((button) => {
    button.addEventListener('click', async () => {
      const value = button.getAttribute('data-copy') || '';
      try { await navigator.clipboard.writeText(value); }
      catch {
        const area = document.createElement('textarea');
        area.value = value;
        area.style.position = 'fixed';
        area.style.opacity = '0';
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
  const activate = (tab) => {
    const target = tab.dataset.terminal;
    tabs.forEach((item) => {
      const active = item === tab;
      item.classList.toggle('active', active);
      item.setAttribute('aria-selected', String(active));
      item.tabIndex = active ? 0 : -1;
    });
    panes.forEach((pane) => {
      const active = pane.dataset.pane === target;
      pane.classList.toggle('active', active);
      pane.hidden = !active;
    });
  };
  tabs.forEach((tab, index) => {
    tab.addEventListener('click', () => activate(tab));
    tab.addEventListener('keydown', (event) => {
      if (!['ArrowLeft','ArrowRight','Home','End'].includes(event.key)) return;
      event.preventDefault();
      const next = event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : event.key === 'ArrowRight' ? (index + 1) % tabs.length : (index - 1 + tabs.length) % tabs.length;
      tabs[next].focus();
      activate(tabs[next]);
    });
  });
  if (tabs[0]) activate(tabs.find((tab) => tab.classList.contains('active')) || tabs[0]);

  document.querySelectorAll('details.tool-menu').forEach((details) => {
    details.addEventListener('toggle', () => {
      if (!details.open) return;
      document.querySelectorAll('details.tool-menu').forEach((other) => {
        if (other !== details) other.removeAttribute('open');
      });
    });
  });
  document.addEventListener('click', (event) => {
    if (!event.target.closest('.tool-menu')) {
      document.querySelectorAll('details.tool-menu').forEach((details) => details.removeAttribute('open'));
    }
  });

  const year = document.getElementById('year');
  if (year) year.textContent = new Date().getFullYear();
})();