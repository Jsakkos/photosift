// Compare screen — 2-up with locked pan/zoom.
// Two variants exposed:
//   mvp  — score pills (sharp / face / eye / smile)
//   lean — minimal metadata (filename + stars + one-line summary)

function CompareScreen({ variant = 'mvp' }) {
  const T = darkTheme;
  const left  = { seed: 'bw-g2-a', name: 'DSC_0418.NEF', stars: 2, scores: { sharp: 92, face: 88, eye: 86, smile: 74 }, picked: true };
  const right = { seed: 'bw-g2-b', name: 'DSC_0419.NEF', stars: 1, scores: { sharp: 84, face: 82, eye: 71, smile: 79 }, picked: false };

  return (
    <div style={{ ...T.root, display: 'flex', flexDirection: 'column' }}>
      <div style={{ padding: '10px 16px', borderBottom: T.border, display: 'flex', alignItems: 'center', gap: 14, fontSize: 11 }}>
        <span style={{ color: T.fg, fontWeight: 500 }}>2-up compare</span>
        <span style={{ color: T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>Group 2 · locked zoom</span>
        <div style={{ flex: 1 }} />
        <div style={{ display: 'flex', gap: 10, fontSize: 11, color: T.fgDim }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>1</Kbd><Kbd>2</Kbd> pick L/R</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>Tab</Kbd> toggle</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>Esc</Kbd> exit</div>
        </div>
      </div>

      <div style={{ flex: 1, display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 2, background: T.borderColor, minHeight: 0 }}>
        <ComparePanel photo={left} side="L" T={T} variant={variant} />
        <ComparePanel photo={right} side="R" T={T} variant={variant} />
      </div>

      <div style={{ padding: '8px 16px', borderTop: T.border, background: T.bg2, display: 'flex', alignItems: 'center', gap: 14, fontSize: 11, color: T.fgDim }}>
        <span>Winner promoted to <span style={{ color: T.accent }}>2★</span></span>
        <div style={{ flex: 1 }} />
        <span style={{ fontFamily: 'JetBrains Mono, monospace' }}>pan + zoom synchronised · eye-level</span>
      </div>
    </div>
  );
}

function ComparePanel({ photo, side, T, variant }) {
  return (
    <div style={{ background: '#0a0a0a', display: 'flex', flexDirection: 'column', position: 'relative', minHeight: 0 }}>
      <div style={{ position: 'absolute', top: 12, left: 12, zIndex: 2, fontSize: 14, fontWeight: 600, color: photo.picked ? T.success : T.fgDim, background: 'rgba(0,0,0,0.55)', padding: '3px 9px', borderRadius: 3, fontFamily: 'JetBrains Mono, monospace' }}>
        {side}
        {photo.picked && <span style={{ marginLeft: 8, fontSize: 10 }}>✓ PICKED</span>}
      </div>
      <div style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 20, minHeight: 0 }}>
        <Photo seed={photo.seed} w="100%" h="100%" style={{ maxHeight: '100%', maxWidth: '100%', objectFit: 'contain' }} />
      </div>
      {variant === 'mvp' ? (
        <div style={{ padding: '10px 14px', background: 'rgba(20,20,20,0.9)', borderTop: T.border, display: 'flex', alignItems: 'center', gap: 14 }}>
          <div>
            <div style={{ fontSize: 11, color: T.fg, fontFamily: 'JetBrains Mono, monospace' }}>{photo.name}</div>
            <div style={{ marginTop: 2 }}><Stars n={photo.stars} size={10} /></div>
          </div>
          <div style={{ flex: 1 }} />
          {Object.entries(photo.scores).map(([k, v]) => (
            <div key={k} style={{ textAlign: 'center' }}>
              <div style={{ fontSize: 9, color: T.fgMute, textTransform: 'uppercase', letterSpacing: 0.8 }}>{k}</div>
              <div style={{ fontSize: 13, fontWeight: 600, color: v >= 85 ? T.accent2 : T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>{v}</div>
            </div>
          ))}
        </div>
      ) : (
        <div style={{ padding: '8px 14px', background: 'rgba(20,20,20,0.9)', borderTop: T.border, display: 'flex', alignItems: 'center', gap: 12 }}>
          <div style={{ fontSize: 11, color: T.fg, fontFamily: 'JetBrains Mono, monospace' }}>{photo.name}</div>
          <Stars n={photo.stars} size={10} />
          <div style={{ flex: 1 }} />
          <div style={{ fontSize: 10, color: T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>
            sharp {photo.scores.sharp} · eye {photo.scores.eye}
          </div>
        </div>
      )}
    </div>
  );
}

window.CompareScreen = CompareScreen;
