(() => {
  const header = document.querySelector('.site-header');
  const progress = document.getElementById('pageProgress');
  const cursorGlow = document.getElementById('cursorGlow');
  const menuButton = document.getElementById('menuButton');
  const navLinks = document.getElementById('navLinks');
  const toast = document.getElementById('toast');
  const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

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
    }, { threshold: 0.12, rootMargin: '0px 0px -35px' });
    reveals.forEach((element) => observer.observe(element));
  } else {
    reveals.forEach((element) => element.classList.add('visible'));
  }

  if (!reduceMotion && window.matchMedia('(hover:hover) and (pointer:fine)').matches) {
    document.querySelectorAll('.tilt-card').forEach((card) => {
      card.addEventListener('pointermove', (event) => {
        const rect = card.getBoundingClientRect();
        const x = (event.clientX - rect.left) / rect.width - 0.5;
        const y = (event.clientY - rect.top) / rect.height - 0.5;
        const rotateX = (-y * 4.2).toFixed(2);
        const rotateY = (x * 5.2).toFixed(2);
        card.style.transform = `perspective(1100px) rotateX(${rotateX}deg) rotateY(${rotateY}deg) translateY(-1px)`;
      });
      card.addEventListener('pointerleave', () => { card.style.transform = ''; });
    });
  }

  let toastTimer;
  const showToast = (message = 'Copied to clipboard') => {
    if (!toast) return;
    toast.textContent = message;
    toast.classList.add('show');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => toast.classList.remove('show'), 1600);
  };

  document.querySelectorAll('.copy-command').forEach((button) => {
    button.addEventListener('click', async () => {
      const value = button.getAttribute('data-copy') || '';
      try {
        await navigator.clipboard.writeText(value);
        showToast();
      } catch {
        const textArea = document.createElement('textarea');
        textArea.value = value;
        textArea.style.position = 'fixed';
        textArea.style.opacity = '0';
        document.body.appendChild(textArea);
        textArea.select();
        document.execCommand('copy');
        textArea.remove();
        showToast();
      }
    });
  });

  const tabs = document.querySelectorAll('.terminal-tab');
  const panes = document.querySelectorAll('.terminal-pane');
  tabs.forEach((tab) => {
    tab.addEventListener('click', () => {
      const target = tab.getAttribute('data-terminal');
      tabs.forEach((item) => item.classList.toggle('active', item === tab));
      panes.forEach((pane) => pane.classList.toggle('active', pane.getAttribute('data-pane') === target));
    });
  });

  document.getElementById('year').textContent = new Date().getFullYear();
})();
