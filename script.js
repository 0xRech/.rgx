(() => {
  const base = document.createElement('script');
  base.src = '/script-base-v6.js?v=20260904-6';
  base.defer = true;
  document.head.appendChild(base);

  const installBenchmarkSnapshots = () => {
    const section = document.getElementById('benchmark');
    if (!section || section.dataset.rgxSnapshots === '1') return;
    section.dataset.rgxSnapshots = '1';

    const english = document.documentElement.lang.toLowerCase().startsWith('en');
    const copy = english
      ? {
          eyebrow: 'Observed cross-platform runs',
          title: 'Real benchmark snapshots',
          intro: 'Two v0.4.0-alpha.2 runs on Windows and macOS. Compare methods inside each card: the machines and input sets are different, so Windows and macOS are not directly comparable.',
          input: 'Input',
          files: 'files',
          dedup: 'Dedup share',
          method: 'Method',
          size: 'Size',
          pack: 'Pack',
          extract: 'Extract',
          packRate: 'Pack MiB/s',
          extractRate: 'Extr MiB/s',
          smaller: 'smaller than ZIP',
          note: 'Single-run wall-clock measurements. 7-Zip was not installed for either run. Results can vary with cache state, storage, CPU, power mode and run order.',
          privateNote: 'RGX Private added only 0.01 MiB versus plain RGX in both runs.'
        }
      : {
          eyebrow: 'Gemessene Cross-Platform-Läufe',
          title: 'Echte Benchmark-Snapshots',
          intro: 'Zwei Läufe mit v0.4.0-alpha.2 unter Windows und macOS. Vergleiche die Methoden innerhalb einer Karte: Hardware und Datensätze unterscheiden sich, daher sind Windows und macOS nicht direkt gegeneinander vergleichbar.',
          input: 'Eingabe',
          files: 'Dateien',
          dedup: 'Dedup-Anteil',
          method: 'Methode',
          size: 'Größe',
          pack: 'Pack',
          extract: 'Extract',
          packRate: 'Pack MiB/s',
          extractRate: 'Extr MiB/s',
          smaller: 'kleiner als ZIP',
          note: 'Einzelne Wall-Clock-Messungen. 7-Zip war bei beiden Läufen nicht installiert. Cache-Zustand, Laufwerk, CPU, Energiemodus und Reihenfolge können die Werte beeinflussen.',
          privateNote: 'RGX Private benötigte in beiden Läufen nur 0,01 MiB mehr als normales RGX.'
        };

    const runs = [
      {
        os: 'Windows',
        platform: 'x86-64',
        input: '1.25 GiB',
        files: '3,113',
        dedup: '5.61%',
        smaller: '9.2%',
        rows: [
          ['RGX', '289.34 MiB', '24.71 s', '15.57 s', '51.9', '82.4'],
          ['RGX Private', '289.35 MiB', '14.08 s', '30.13 s', '91.1', '42.6'],
          ['ZIP (Deflate)', '318.65 MiB', '37.95 s', '34.26 s', '33.8', '37.5']
        ]
      },
      {
        os: 'macOS',
        platform: 'Apple Silicon',
        input: '1.09 GiB',
        files: '3,413',
        dedup: '20.56%',
        smaller: '18.3%',
        rows: [
          ['RGX', '248.82 MiB', '10.73 s', '2.12 s', '104.3', '526.9'],
          ['RGX Private', '248.83 MiB', '11.01 s', '5.11 s', '101.7', '219.0'],
          ['ZIP (Deflate)', '304.49 MiB', '22.03 s', '2.95 s', '50.8', '379.1']
        ]
      }
    ];

    const style = document.createElement('style');
    style.textContent = `
      .rgx-benchmark-snapshots{margin-top:56px}
      .rgx-benchmark-snapshots .section-heading{max-width:780px;margin:0 auto 28px;text-align:center}
      .bench-snapshot-grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:20px}
      .bench-snapshot-card{padding:0;overflow:hidden}
      .bench-snapshot-head{display:flex;align-items:flex-start;justify-content:space-between;gap:18px;padding:24px 24px 18px;border-bottom:1px solid var(--line,rgba(255,255,255,.1))}
      .bench-os{display:flex;align-items:center;gap:10px;font-weight:800;font-size:1.08rem}
      .bench-os-dot{width:10px;height:10px;border-radius:50%;background:linear-gradient(135deg,#7cf7c6,#9b8cff);box-shadow:0 0 22px rgba(124,247,198,.45)}
      .bench-platform{display:block;margin-top:5px;color:var(--muted,#9ca3af);font-size:.82rem;font-weight:600}
      .bench-version{font-family:var(--mono,ui-monospace,SFMono-Regular,Menlo,monospace);font-size:.72rem;color:var(--muted,#9ca3af);white-space:nowrap}
      .bench-meta{display:grid;grid-template-columns:repeat(3,1fr);gap:1px;background:var(--line,rgba(255,255,255,.1));border-bottom:1px solid var(--line,rgba(255,255,255,.1))}
      .bench-meta div{padding:16px 18px;background:var(--card,#11141d)}
      .bench-meta span{display:block;color:var(--muted,#9ca3af);font-size:.7rem;text-transform:uppercase;letter-spacing:.08em;margin-bottom:5px}
      .bench-meta strong{font-size:.92rem}
      .bench-table-scroll{overflow-x:auto}
      .bench-snapshot-table{width:100%;border-collapse:collapse;min-width:650px;font-size:.8rem}
      .bench-snapshot-table th,.bench-snapshot-table td{padding:12px 14px;text-align:right;border-bottom:1px solid var(--line,rgba(255,255,255,.08));white-space:nowrap}
      .bench-snapshot-table th{color:var(--muted,#9ca3af);font-size:.66rem;letter-spacing:.06em;text-transform:uppercase;font-weight:700}
      .bench-snapshot-table th:first-child,.bench-snapshot-table td:first-child{text-align:left}
      .bench-snapshot-table tbody tr:last-child td{border-bottom:0}
      .bench-snapshot-table tr.rgx-private td:first-child{color:#8ee8c4;font-weight:800}
      .bench-snapshot-foot{display:flex;align-items:center;justify-content:space-between;gap:16px;padding:18px 22px;background:linear-gradient(90deg,rgba(124,247,198,.07),rgba(155,140,255,.05))}
      .bench-win{font-size:.78rem;color:var(--muted,#9ca3af)}
      .bench-win strong{display:block;color:var(--text,#fff);font-size:1.25rem;margin-bottom:2px}
      .bench-private-note{max-width:430px;text-align:right;color:var(--muted,#9ca3af);font-size:.74rem;line-height:1.5}
      .bench-methodology{margin:18px auto 0;max-width:980px;color:var(--muted,#9ca3af);font-size:.78rem;line-height:1.6;text-align:center}
      @media (max-width:980px){.bench-snapshot-grid{grid-template-columns:1fr}}
      @media (max-width:620px){.rgx-benchmark-snapshots{margin-top:38px}.bench-snapshot-head{padding:20px}.bench-meta{grid-template-columns:1fr}.bench-meta div{padding:12px 18px}.bench-snapshot-foot{align-items:flex-start;flex-direction:column}.bench-private-note{text-align:left}}
    `;
    document.head.appendChild(style);

    const cards = runs.map((run) => {
      const rows = run.rows.map((row, index) => `
        <tr class="${index === 1 ? 'rgx-private' : ''}">
          <td><strong>${row[0]}</strong></td>
          <td>${row[1]}</td>
          <td>${row[2]}</td>
          <td>${row[3]}</td>
          <td>${row[4]}</td>
          <td>${row[5]}</td>
        </tr>`).join('');

      return `
        <article class="benchmark-card bench-snapshot-card">
          <div class="bench-snapshot-head">
            <div><div class="bench-os"><span class="bench-os-dot"></span>${run.os}</div><span class="bench-platform">${run.platform}</span></div>
            <span class="bench-version">v0.4.0-alpha.2</span>
          </div>
          <div class="bench-meta">
            <div><span>${copy.input}</span><strong>${run.input}</strong></div>
            <div><span>${copy.files}</span><strong>${run.files}</strong></div>
            <div><span>${copy.dedup}</span><strong>${run.dedup}</strong></div>
          </div>
          <div class="bench-table-scroll">
            <table class="bench-snapshot-table">
              <thead><tr><th>${copy.method}</th><th>${copy.size}</th><th>${copy.pack}</th><th>${copy.extract}</th><th>${copy.packRate}</th><th>${copy.extractRate}</th></tr></thead>
              <tbody>${rows}</tbody>
            </table>
          </div>
          <div class="bench-snapshot-foot">
            <div class="bench-win"><strong>${run.smaller}</strong>${copy.smaller}</div>
            <div class="bench-private-note">${copy.privateNote}</div>
          </div>
        </article>`;
    }).join('');

    const wrapper = document.createElement('div');
    wrapper.className = 'shell rgx-benchmark-snapshots';
    wrapper.innerHTML = `
      <div class="section-heading">
        <div class="eyebrow">${copy.eyebrow}</div>
        <h2>${copy.title}</h2>
        <p>${copy.intro}</p>
      </div>
      <div class="bench-snapshot-grid">${cards}</div>
      <p class="bench-methodology">${copy.note}</p>`;

    section.appendChild(wrapper);
  };

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', installBenchmarkSnapshots, { once: true });
  } else {
    installBenchmarkSnapshots();
  }
})();
