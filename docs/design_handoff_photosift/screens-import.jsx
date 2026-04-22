// Import screen — shows ingest in progress
// Layout: left rail (sources), center (file list / progress), right (stats)

function ImportScreen({ theme = 'dark' }) {
  const T = theme === 'dark' ? darkTheme : lightTheme;
  const files = [
    { name: 'DSC_0412.NEF', size: '42.3 MB', progress: 1.0, group: 'g1' },
    { name: 'DSC_0413.NEF', size: '41.8 MB', progress: 1.0, group: 'g1' },
    { name: 'DSC_0414.NEF', size: '43.1 MB', progress: 1.0, group: 'g1' },
    { name: 'DSC_0415.NEF', size: '42.9 MB', progress: 1.0, group: 'g2' },
    { name: 'DSC_0416.NEF', size: '42.7 MB', progress: 1.0, group: 'g2' },
    { name: 'DSC_0417.NEF', size: '43.0 MB', progress: 0.68, group: 'g3' },
    { name: 'DSC_0418.NEF', size: '42.5 MB', progress: 0.22, group: 'g3' },
    { name: 'DSC_0419.NEF', size: '—', progress: 0, group: 'g3' },
    { name: 'DSC_0420.NEF', size: '—', progress: 0, group: 'g4' },
    { name: 'DSC_0421.NEF', size: '—', progress: 0, group: 'g4' },
    { name: 'DSC_0422.NEF', size: '—', progress: 0, group: 'g4' },
    { name: 'DSC_0423.NEF', size: '—', progress: 0, group: 'g5' },
  ];

  return (
    <div style={{ ...T.root, display: 'grid', gridTemplateColumns: '240px 1fr 300px', height: '100%' }}>
      {/* Left rail — sources */}
      <div style={{ ...T.rail, padding: '20px 16px', borderRight: T.border }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 24 }}>
          <LogoB size={22} color={T.accent} />
          <span style={{ fontSize: 13, fontWeight: 600, letterSpacing: -0.2, color: T.fg }}>Photosift</span>
        </div>
        <div style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: 1.2, color: T.fgDim, marginBottom: 10 }}>Source</div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '10px 8px', background: T.hover, borderRadius: 4, marginBottom: 4 }}>
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke={T.fg} strokeWidth="1.3"><rect x="2" y="4" width="12" height="9" rx="1.5" /><circle cx="8" cy="8.5" r="2" /></svg>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12, color: T.fg, fontWeight: 500 }}>SD Card</div>
            <div style={{ fontSize: 10, color: T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>SANDISK 64G · 64 files</div>
          </div>
        </div>
        <div style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: 1.2, color: T.fgDim, margin: '20px 0 10px' }}>Destination</div>
        <div style={{ fontSize: 11, fontFamily: 'JetBrains Mono, monospace', color: T.fgDim, lineHeight: 1.6 }}>
          <div>~/Pictures/</div>
          <div style={{ paddingLeft: 10, color: T.fg }}>2026/</div>
          <div style={{ paddingLeft: 20, color: T.accent }}>2026-04_Spring-Show/</div>
          <div style={{ paddingLeft: 30, color: T.fgDim }}>RAW/</div>
        </div>

        <div style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: 1.2, color: T.fgDim, margin: '20px 0 10px' }}>Pipeline</div>
        <PipelineStep label="Copy RAW" state="done" />
        <PipelineStep label="Extract preview" state="done" />
        <PipelineStep label="Read EXIF" state="done" />
        <PipelineStep label="p-hash group" state="running" />
        <PipelineStep label="Sharpness score" state="queued" />
        <PipelineStep label="Face + eye detect" state="queued" />
      </div>

      {/* Center — file list with per-file progress */}
      <div style={{ padding: '20px 24px', overflow: 'hidden', display: 'flex', flexDirection: 'column' }}>
        <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', marginBottom: 4 }}>
          <div>
            <div style={{ fontSize: 22, fontWeight: 600, letterSpacing: -0.5, color: T.fg }}>Ingesting <span style={{ color: T.accent }}>Spring Show</span></div>
            <div style={{ fontSize: 12, color: T.fgDim, marginTop: 2, fontFamily: 'JetBrains Mono, monospace' }}>
              5 / 64 files · 5 groups detected · 00:42 elapsed · ~02:15 remaining
            </div>
          </div>
          <button style={{ ...T.btnGhost, fontSize: 11 }}>Cancel</button>
        </div>

        {/* Stacked progress: one line per file, grouped by burst */}
        <div style={{ marginTop: 20, flex: 1, overflow: 'auto' }}>
          {files.map((f, i) => {
            const prev = i > 0 ? files[i-1] : null;
            const isNewGroup = !prev || prev.group !== f.group;
            return (
              <React.Fragment key={i}>
                {isNewGroup && (
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8, margin: '14px 0 6px', fontFamily: 'JetBrains Mono, monospace', fontSize: 10, color: T.fgDim, textTransform: 'uppercase', letterSpacing: 1 }}>
                    <div style={{ width: 14, height: 1, background: T.borderColor }} />
                    Group {f.group.toUpperCase()} · burst
                    <div style={{ flex: 1, height: 1, background: T.borderColor }} />
                  </div>
                )}
                <div style={{ display: 'grid', gridTemplateColumns: '32px 1fr 64px 60px', gap: 12, alignItems: 'center', padding: '4px 0', fontSize: 11, fontFamily: 'JetBrains Mono, monospace' }}>
                  <div style={{ color: T.fgDim }}>{String(i+1).padStart(3, '0')}</div>
                  <div style={{ color: f.progress > 0 ? T.fg : T.fgDim }}>
                    {f.name}
                    {f.progress > 0 && f.progress < 1 && (
                      <div style={{ height: 2, background: T.borderColor, marginTop: 3, borderRadius: 1, overflow: 'hidden' }}>
                        <div style={{ height: '100%', width: `${f.progress * 100}%`, background: T.accent }} />
                      </div>
                    )}
                  </div>
                  <div style={{ color: T.fgDim, textAlign: 'right' }}>{f.size}</div>
                  <div style={{ textAlign: 'right', color: f.progress === 1 ? T.success : f.progress > 0 ? T.accent : T.fgDim }}>
                    {f.progress === 1 ? '✓ done' : f.progress > 0 ? `${Math.round(f.progress*100)}%` : 'queued'}
                  </div>
                </div>
              </React.Fragment>
            );
          })}
        </div>
      </div>

      {/* Right — live stats */}
      <div style={{ padding: '20px 20px', borderLeft: T.border, background: T.rail.background }}>
        <div style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: 1.2, color: T.fgDim, marginBottom: 12 }}>Live</div>
        <StatRow label="Copied" value="217 MB" sub="of 2.7 GB" accent={T.accent} />
        <StatRow label="Read speed" value="48.2" unit="MB/s" />
        <StatRow label="Files" value="5" sub="of 64" />
        <StatRow label="Groups" value="5" sub="detected" />

        <div style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: 1.2, color: T.fgDim, margin: '28px 0 12px' }}>AI Queue</div>
        <div style={{ background: T.hover, borderRadius: 4, padding: 12 }}>
          <div style={{ fontSize: 11, color: T.fg, marginBottom: 8, display: 'flex', justifyContent: 'space-between' }}>
            <span>Sharpness</span><span style={{ fontFamily: 'JetBrains Mono, monospace', color: T.fgDim }}>0 / 64</span>
          </div>
          <div style={{ fontSize: 11, color: T.fg, marginBottom: 8, display: 'flex', justifyContent: 'space-between' }}>
            <span>Faces</span><span style={{ fontFamily: 'JetBrains Mono, monospace', color: T.fgDim }}>0 / 64</span>
          </div>
          <div style={{ fontSize: 11, color: T.fg, display: 'flex', justifyContent: 'space-between' }}>
            <span>Eyes · smile</span><span style={{ fontFamily: 'JetBrains Mono, monospace', color: T.fgDim }}>0 / 64</span>
          </div>
          <div style={{ fontSize: 10, color: T.fgDim, marginTop: 10, paddingTop: 10, borderTop: T.border, fontFamily: 'JetBrains Mono, monospace' }}>
            starts when copy completes
          </div>
        </div>

        <div style={{ position: 'absolute', bottom: 20, right: 20, left: 'auto', display: 'flex', gap: 6 }}>
          <Kbd>⌘</Kbd><Kbd>⏎</Kbd>
          <span style={{ fontSize: 11, color: T.fgDim, marginLeft: 6 }}>begin triage when ready</span>
        </div>
      </div>
    </div>
  );
}

function PipelineStep({ label, state }) {
  const T = darkTheme;
  const color = state === 'done' ? T.success : state === 'running' ? T.accent : T.fgDim;
  const icon = state === 'done'
    ? <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke={color} strokeWidth="1.5"><path d="M2 5l2 2 4-4" /></svg>
    : state === 'running'
    ? <div style={{ width: 7, height: 7, borderRadius: 4, background: color, boxShadow: `0 0 8px ${color}` }} />
    : <div style={{ width: 7, height: 7, borderRadius: 4, border: `1px solid ${color}` }} />;
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '3px 0', fontSize: 11 }}>
      <div style={{ width: 10, display: 'flex', justifyContent: 'center' }}>{icon}</div>
      <span style={{ color: state === 'queued' ? T.fgDim : T.fg, fontFamily: 'JetBrains Mono, monospace' }}>{label}</span>
    </div>
  );
}

function StatRow({ label, value, unit, sub, accent }) {
  const T = darkTheme;
  return (
    <div style={{ marginBottom: 14 }}>
      <div style={{ fontSize: 10, color: T.fgDim, textTransform: 'uppercase', letterSpacing: 1, marginBottom: 2 }}>{label}</div>
      <div style={{ fontFamily: 'JetBrains Mono, monospace', color: accent || T.fg, fontSize: 18, fontWeight: 500, letterSpacing: -0.3 }}>
        {value}{unit && <span style={{ fontSize: 11, color: T.fgDim, marginLeft: 4 }}>{unit}</span>}
      </div>
      {sub && <div style={{ fontSize: 10, color: T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>{sub}</div>}
    </div>
  );
}

window.ImportScreen = ImportScreen;
