(() => {
  const root = document.documentElement;
  const header = document.querySelector('.site-header');
  const progress = document.getElementById('pageProgress');
  const cursorGlow = document.getElementById('cursorGlow');
  const menuButton = document.getElementById('menuButton');
  const navLinks = document.getElementById('navLinks');
  const toast = document.getElementById('toast');
  const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  const applyTheme = (theme) => {
    if (theme === 'light' || theme === 'dark') root.dataset.theme = theme;
    else root.removeAttribute('data-theme');
    localStorage.setItem('rgx-theme', theme);
    document.querySelectorAll('[data-theme-choice]').forEach((b) => b.setAttribute('aria-current', String(b.dataset.themeChoice === theme)));
    const label = document.getElementById('themeLabel');
    if (label) label.textContent = theme === 'light' ? 'Light' : theme === 'dark' ? 'Dark' : 'System';
  };
  applyTheme(localStorage.getItem('rgx-theme') || 'system');
  document.querySelectorAll('[data-theme-choice]').forEach((b) => b.addEventListener('click', () => { applyTheme(b.dataset.themeChoice); b.closest('details')?.removeAttribute('open'); }));
  document.querySelectorAll('[data-lang-choice]').forEach((a) => a.addEventListener('click', () => localStorage.setItem('rgx-lang', a.dataset.langChoice)));

  const onScroll = () => {
    const y = window.scrollY;
    header?.classList.toggle('scrolled', y > 12);
    const max = Math.max(document.documentElement.scrollHeight - window.innerHeight, 1);
    if (progress) progress.style.width = `${Math.min((y / max) * 100, 100)}%`;
  };
  onScroll(); window.addEventListener('scroll', onScroll, { passive: true });

  if (!reduceMotion && window.matchMedia('(pointer:fine)').matches) {
    document.body.classList.add('pointer-active');
    window.addEventListener('pointermove', (event) => { if (cursorGlow) { cursorGlow.style.left=`${event.clientX}px`; cursorGlow.style.top=`${event.clientY}px`; } }, { passive:true });
  }
  menuButton?.addEventListener('click', () => { const open=menuButton.getAttribute('aria-expanded')==='true'; menuButton.setAttribute('aria-expanded',String(!open)); navLinks?.classList.toggle('open',!open); });
  navLinks?.querySelectorAll('a').forEach((link)=>link.addEventListener('click',()=>{navLinks.classList.remove('open');menuButton?.setAttribute('aria-expanded','false');}));

  const reveals=document.querySelectorAll('.reveal');
  reveals.forEach((e)=>{const d=e.getAttribute('data-delay');if(d)e.style.setProperty('--delay',`${d}ms`);});
  if('IntersectionObserver' in window && !reduceMotion){const o=new IntersectionObserver((entries)=>entries.forEach((x)=>{if(x.isIntersecting){x.target.classList.add('visible');o.unobserve(x.target);}}),{threshold:.12,rootMargin:'0px 0px -35px'});reveals.forEach((e)=>o.observe(e));} else reveals.forEach((e)=>e.classList.add('visible'));

  if(!reduceMotion && window.matchMedia('(hover:hover) and (pointer:fine)').matches){document.querySelectorAll('.tilt-card').forEach((card)=>{card.addEventListener('pointermove',(event)=>{const r=card.getBoundingClientRect(),x=(event.clientX-r.left)/r.width-.5,y=(event.clientY-r.top)/r.height-.5;card.style.transform=`perspective(1100px) rotateX(${(-y*4.2).toFixed(2)}deg) rotateY(${(x*5.2).toFixed(2)}deg) translateY(-1px)`;});card.addEventListener('pointerleave',()=>card.style.transform='');});}

  let toastTimer; const showToast=(message)=>{if(!toast)return;toast.textContent=message||toast.dataset.default||'Copied';toast.classList.add('show');clearTimeout(toastTimer);toastTimer=setTimeout(()=>toast.classList.remove('show'),1600);};
  document.querySelectorAll('.copy-command').forEach((button)=>button.addEventListener('click',async()=>{const value=button.getAttribute('data-copy')||'';try{await navigator.clipboard.writeText(value);}catch{const t=document.createElement('textarea');t.value=value;t.style.position='fixed';t.style.opacity='0';document.body.appendChild(t);t.select();document.execCommand('copy');t.remove();}showToast();}));

  const tabs=[...document.querySelectorAll('[role="tab"]')], panes=[...document.querySelectorAll('[role="tabpanel"]')];
  const activate=(tab)=>{const target=tab.dataset.terminal;tabs.forEach((t)=>{const on=t===tab;t.classList.toggle('active',on);t.setAttribute('aria-selected',String(on));t.tabIndex=on?0:-1;});panes.forEach((p)=>{const on=p.dataset.pane===target;p.classList.toggle('active',on);p.hidden=!on;});};
  tabs.forEach((tab,i)=>{tab.addEventListener('click',()=>activate(tab));tab.addEventListener('keydown',(e)=>{if(!['ArrowLeft','ArrowRight','Home','End'].includes(e.key))return;e.preventDefault();let n=e.key==='Home'?0:e.key==='End'?tabs.length-1:e.key==='ArrowRight'?(i+1)%tabs.length:(i-1+tabs.length)%tabs.length;tabs[n].focus();activate(tabs[n]);});});
  if(tabs[0]) activate(tabs.find(t=>t.classList.contains('active'))||tabs[0]);

  document.querySelectorAll('details.tool-menu').forEach((d)=>d.addEventListener('toggle',()=>{if(d.open)document.querySelectorAll('details.tool-menu').forEach((o)=>{if(o!==d)o.removeAttribute('open');});}));
  document.addEventListener('click',(e)=>{if(!e.target.closest('.tool-menu'))document.querySelectorAll('details.tool-menu').forEach((d)=>d.removeAttribute('open'));});
  const year=document.getElementById('year'); if(year) year.textContent=new Date().getFullYear();
})();