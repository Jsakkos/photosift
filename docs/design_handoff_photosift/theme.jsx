// Theme + app chrome for Windows 11
const darkTheme = {
  name: 'dark',
  bg: '#151515',
  bg2: '#1c1c1c',
  bg3: '#232323',
  fg: '#e8e6e2',
  fgDim: 'rgba(232,230,226,0.55)',
  fgMute: 'rgba(232,230,226,0.32)',
  accent: '#d4a574',
  accent2: '#7fb8d9',
  accentBlue: '#4a82d9',
  success: '#6fbb7b',
  danger: '#d97a7a',
  warning: '#e8c64a',
  borderColor: 'rgba(232,230,226,0.07)',
  border: '1px solid rgba(232,230,226,0.07)',
  hover: 'rgba(232,230,226,0.04)',
  selected: 'rgba(74,130,217,0.18)',
  root: {
    background: '#151515',
    color: '#e8e6e2',
    fontFamily: '"Segoe UI Variable", "Segoe UI", system-ui, sans-serif',
    fontSize: 13,
    height: '100%',
    position: 'relative',
  },
  rail: { background: '#111111' },
  btnGhost: {
    background: 'transparent', border: '1px solid rgba(232,230,226,0.12)',
    color: 'rgba(232,230,226,0.85)', padding: '5px 12px',
    borderRadius: 4, cursor: 'pointer', fontFamily: 'inherit', fontSize: 12,
  },
  btnAccent: {
    background: '#4a82d9', border: 'none', color: '#fff',
    padding: '6px 14px', borderRadius: 4, cursor: 'pointer',
    fontFamily: 'inherit', fontWeight: 500, fontSize: 12,
  },
};

// Windows 11 title bar
function Win11Chrome({ children, title = 'Photosift', stage, project }) {
  const T = darkTheme;
  const stages = [
    { id: 'library', label: 'Library' },
    { id: 'triage',  label: 'Triage' },
    { id: 'select',  label: 'Select' },
    { id: 'route',   label: 'Route' },
  ];
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', background: T.bg }}>
      {/* Titlebar */}
      <div style={{ height: 32, display: 'flex', alignItems: 'stretch', flexShrink: 0, background: T.bg2, borderBottom: T.border }}>
        <div style={{ display: 'flex', alignItems: 'center', padding: '0 12px', gap: 10 }}>
          <LogoB size={16} color={T.accent} />
          <span style={{ fontSize: 12, color: T.fgDim }}>{title}{project && <span style={{ color: T.fgMute }}> — {project}</span>}</span>
        </div>
        <div style={{ flex: 1, WebkitAppRegion: 'drag' }} />
        {/* Windows 11 caption buttons */}
        <div style={{ display: 'flex' }}>
          <CaptionBtn kind="min" />
          <CaptionBtn kind="max" />
          <CaptionBtn kind="close" />
        </div>
      </div>
      {/* Tab bar / workflow */}
      {stage && (
        <div style={{ height: 40, display: 'flex', alignItems: 'center', padding: '0 16px', gap: 2, background: T.bg, borderBottom: T.border, flexShrink: 0 }}>
          {stages.map((s, i) => (
            <div key={s.id} style={{
              padding: '8px 14px', fontSize: 12, fontWeight: 500,
              color: s.id === stage ? T.fg : T.fgDim,
              borderBottom: s.id === stage ? `2px solid ${T.accentBlue}` : '2px solid transparent',
              marginBottom: -1, cursor: 'pointer',
              display: 'flex', alignItems: 'center', gap: 8,
            }}>
              <span style={{ fontFamily: 'JetBrains Mono, monospace', fontSize: 10, color: T.fgMute }}>{i+1}</span>
              {s.label}
            </div>
          ))}
          <div style={{ flex: 1 }} />
          <span style={{ fontSize: 11, color: T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>{project}</span>
        </div>
      )}
      <div style={{ flex: 1, minHeight: 0, position: 'relative' }}>{children}</div>
    </div>
  );
}

function CaptionBtn({ kind }) {
  const T = darkTheme;
  const paths = {
    min: <path d="M3 6h6" stroke="currentColor" strokeWidth="1" />,
    max: <rect x="3" y="3" width="6" height="6" stroke="currentColor" strokeWidth="1" fill="none" />,
    close: <g><path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" strokeWidth="1" /></g>,
  };
  return (
    <div style={{
      width: 46, display: 'flex', alignItems: 'center', justifyContent: 'center',
      color: T.fgDim, cursor: 'pointer',
    }}>
      <svg width="12" height="12" viewBox="0 0 12 12">{paths[kind]}</svg>
    </div>
  );
}

Object.assign(window, { darkTheme, Win11Chrome });
