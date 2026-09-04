(() => {
  const root = document.documentElement;
  const header = document.querySelector('.site-header');
  const progress = document.getElementById('pageProgress');
  const cursorGlow = document.getElementById('cursorGlow');
  const menuButton = document.getElementById('menuButton');
  const navLinks = document.getElementById('navLinks');
  const toast = document.getElementById('toast');
  const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const LOGO = '/assets/rgx-logo.png?v=20260904-6';

  /* RGX website is deliberately dark-only. */
  root.dataset.theme = 'dark';
  root.dataset.themeMode = 'dark';
  document.querySelector('meta[name="theme-color"]')?.setAttribute('content', '#08090f');
  document.querySelector('meta[name="color-scheme"]')?.setAttribute('content', 'dark');
  try { localStorage.removeItem('rgx-theme'); } catch {}

  /* Shared final styling for branding and layout. */
  const style = document.createElement('style');
  style.id = 'rgx-final-runtime';
  style.textContent = `
    html,body{color-scheme:dark!important;background:#08090f!important}
    .tool-menu:has([data-theme-choice]),[data-theme-choice],#themeLabel{display:none!important}

    header .brand{
      box-sizing:border-box!important;
      width:172px!important;
      min-height:62px!important;
      height:auto!important;
      display:flex!important;
      align-items:center!important;
      justify-content:center!important;
      padding:8px 10px!important;
      border-radius:13px!important;
      overflow:visible!important;
      background:linear-gradient(145deg,#f6f8fd,#e8edf7)!important;
      border:1px solid rgba(255,255,255,.20)!important;
      box-shadow:0 7px 22px rgba(0,0,0,.14)!important;
    }
    footer .brand{
      box-sizing:border-box!important;
      width:196px!important;
      min-height:76px!important;
      height:auto!important;
      display:flex!important;
      align-items:center!important;
      justify-content:center!important;
      padding:9px 11px!important;
      border-radius:14px!important;
      overflow:visible!important;
      background:linear-gradient(145deg,#f6f8fd,#e8edf7)!important;
      border:1px solid rgba(255,255,255,.18)!important;
      box-shadow:none!important;
    }
    header .brand img.rgx-wordmark,
    footer .brand img.rgx-wordmark{
      display:block!important;
      width:88%!important;
      height:auto!important;
      max-width:88%!important;
      max-height:none!important;
      object-fit:contain!important;
      object-position:center!important;
      transform:none!important;
      flex:0 0 auto!important;
    }

    .official-logo-plate{
      box-sizing:border-box!important;
      width:min(440px,88%)!important;
      min-height:0!important;
      height:auto!important;
      padding:22px 24px!important;
      display:flex!important;
      align-items:center!important;
      justify-content:center!important;
      border-radius:22px!important;
      background:linear-gradient(145deg,#f6f8fd,#e8edf7)!important;
      border:1px solid rgba(255,255,255,.18)!important;
      box-shadow:0 18px 50px rgba(0,0,0,.20)!important;
      overflow:visible!important;
    }
    .official-logo-plate img.rgx-wordmark{
      display:block!important;
      width:84%!important;
      height:auto!important;
      max-width:84%!important;
      max-height:none!important;
      object-fit:contain!important;
      object-position:center!important;
      transform:none!important;
      flex:0 0 auto!important;
    }

    .feature-card.large{min-height:0!important;grid-row:auto!important}
    .feature-card.large .chunk-demo{position:relative!important;left:auto!important;right:auto!important;bottom:auto!important;margin-top:34px!important;min-height:245px!important}
    .feature-grid{align-items:stretch!important}
    .feature-card{min-height:0!important}
    .engine-flow{margin-top:48px!important}
    .engine-node{border:1px solid var(--line)!important;border-radius:18px!important;background:rgba(255,255,255,.012)!important}
    .engine-connector{width:34px!important}
    .final-cta{min-height:0!important;padding-top:72px!important;padding-bottom:72px!important}
    .site-footer{padding-top:64px!important}

    @media(max-width:820px){
      header .brand{width:154px!important;min-height:56px!important;padding:7px 9px!important}
      footer .brand{width:176px!important;min-height:68px!important}
      .official-logo-plate{width:min(400px,90%)!important;padding:18px 20px!important}
      .official-logo-plate img.rgx-wordmark{width:82%!important;max-width:82%!important}
      .feature-card.large .chunk-demo{min-height:220px!important}
      .engine-flow{margin-top:36px!important}
    }
    @media(max-width:560px){
      header .brand{width:140px!important;min-height:52px!important;padding:6px 8px!important}
      .official-logo-plate{width:92%!important;padding:16px 18px!important;border-radius:18px!important}
      .official-logo-plate img.rgx-wordmark{width:80%!important;max-width:80%!important}
    }
  `;
  document.head.appendChild(style);

  /* Remove every old theme selector, independently of its position in the nav. */
  document.querySelectorAll('details.tool-menu').forEach((details) => {
    if (details.querySelector('[data-theme-choice]') || details.querySelector('#themeLabel')) details.remove();
  });

  /* Put the exact same local wordmark in header and footer on every page. */
  document.querySelectorAll('header .brand, footer .brand').forEach((brand) => {
    brand.replaceChildren();
    const img = document.createElement('img');
    img.className = 'rgx-wordmark';
    img.src = LOGO;
    img.alt = 'RGX';
    img.decoding = 'async';
    brand.appendChild(img);
  });

  /* Replace all large logo plates as well. */
  document.querySelectorAll('.official-logo-plate').forEach((plate) => {
    plate.replaceChildren();
    const img = document.createElement('img');
    img.className = 'rgx-wordmark';
    img.src = LOGO;
    img.alt = 'RGX';
    img.decoding = 'async';
    plate.appendChild(img);
  });

  /* Any remaining legacy logo image is redirected to the local PNG. */
  document.querySelectorAll('img').forEach((img) => {
    const src = img.getAttribute('src') || '';
    if (src.includes('user-attachments') || src.includes('rgx-logo.webp') || src.includes('rgx-mark.webp')) {
      if (!img.closest('.brand') && !img.closest('.official-logo-plate')) img.src = LOGO;
    }
  });

  const safeStorage = {
    set(key, value) {
      try { localStorage.setItem(key, value); } catch {}
    }
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

  navLinks?.querySelectorAll('a').forEach((link) => {
    link.addEventListener('click', () => {
      navLinks.classList.remove('open');
      menuButton?.setAttribute('aria-expanded', 'false');
    });
  });

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
  } else reveals.forEach((element) => element.classList.add('visible'));

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
        area.value = value; area.style.position = 'fixed'; area.style.opacity = '0';
        document.body.appendChild(area); area.select(); document.execCommand('copy'); area.remove();
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
      tabs[next].focus(); activate(tabs[next]);
    });
  });
  if (tabs[0]) activate(tabs.find((tab) => tab.classList.contains('active')) || tabs[0]);

  document.querySelectorAll('details.tool-menu').forEach((details) => {
    details.addEventListener('toggle', () => {
      if (!details.open) return;
      document.querySelectorAll('details.tool-menu').forEach((other) => { if (other !== details) other.removeAttribute('open'); });
    });
  });
  document.addEventListener('click', (event) => {
    if (!event.target.closest('.tool-menu')) document.querySelectorAll('details.tool-menu').forEach((details) => details.removeAttribute('open'));
  });

  const year = document.getElementById('year');
  if (year) year.textContent = new Date().getFullYear();
})();