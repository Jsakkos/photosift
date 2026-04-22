// Route screen v2
function RouteScreenV2() {
  const T = darkTheme;
  const picks = Array.from({ length: 18 }).map((_, i) => ({
    seed: `r-${i}`,
    stars: [3,3,2,3,2,2,3,2,2,3,2,2,3,2,2,3,2,2][i],
    routed: ['c1', null, 'c1', 'dxo', null, 'pub', 'c1', null, null, 'c1', 'dxo', null, 'c1', null, 'pub', null, null, 'c1'][i],
  }));
  return (
    <div style={{ ...T.root, display: 'grid', gridTemplateColumns: '1fr 300px' }}>
      <div style={{ padding: 16, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', marginBottom: 14 }}>
          <div>
            <div style={{ fontSize: 9, textTransform: 'uppercase', letterSpacing: 1.4, color: T.fgDim }}>Route</div>
            <div style={{ fontSize: 18, fontWeight: 600, color: T.fg, marginTop: 2 }}>
              <span style={{ color: T.accent, fontFamily: 'JetBrains Mono, monospace' }}>★ ≥ 2</span> · 18 picks ready
            </div>
          </div>
          <div style={{ display: 'inline-flex', background: T.bg2, padding: 2, borderRadius: 4, gap: 1 }}>
            {['all', '★≥1', '★≥2', '★≥3', '★≥4', '★≥5'].map((n, i) => (
              <div key={i} style={{ padding: '4px 10px', fontSize: 10, fontFamily: 'JetBrains Mono, monospace', borderRadius: 3, color: i === 2 ? T.bg : T.fgDim, background: i === 2 ? T.accent : 'transparent' }}>{n}</div>
            ))}
          </div>
        </div>
        <div style={{ flex: 1, overflow: 'auto', display: 'grid', gridTemplateColumns: 'repeat(5, 1fr)', gap: 10, alignContent: 'start' }}>
          {picks.map((p, i) => (
            <div key={i} style={{ position: 'relative' }}>
              <Photo seed={p.seed} w="100%" h={110} />
              <div style={{ position: 'absolute', top: 4, left: 4, background: 'rgba(0,0,0,0.6)', padding: '2px 5px', borderRadius: 2 }}>
                <Stars n={p.stars} size={9} />
              </div>
              {p.routed && (
                <div style={{ position: 'absolute', bottom: 4, right: 4, fontSize: 9, fontFamily: 'JetBrains Mono, monospace', color: T.accent, background: 'rgba(0,0,0,0.7)', padding: '2px 5px', borderRadius: 2 }}>
                  → {p.routed === 'c1' ? 'C1' : p.routed === 'dxo' ? 'DxO' : 'Pub'}
                </div>
              )}
            </div>
          ))}
        </div>
        <div style={{ marginTop: 12, padding: '8px 12px', borderTop: T.border, display: 'flex', gap: 14, fontSize: 11, color: T.fgDim }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>E</Kbd> Capture One</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>⌘</Kbd><Kbd>E</Kbd> DxO</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>D</Kbd> publish JPEG</div>
          <div style={{ flex: 1 }} />
          <span style={{ fontFamily: 'JetBrains Mono, monospace' }}>8 routed · 10 pending</span>
        </div>
      </div>

      <div style={{ borderLeft: T.border, background: T.rail.background, padding: 16, display: 'flex', flexDirection: 'column', gap: 10 }}>
        <div style={{ fontSize: 9, textTransform: 'uppercase', letterSpacing: 1.2, color: T.fgDim }}>Destinations</div>
        <DestCard name="Capture One Pro" sub="opens selected RAWs · reads XMP" kbd="E" count={5} />
        <DestCard name="DxO PhotoLab 7" sub="opens selected RAWs" kbd="⌘E" count={2} />
        <DestCard name="Direct publish" sub="cached JPEG → ~/Dropbox/out/" kbd="D" count={1} />
        <div style={{ fontSize: 9, textTransform: 'uppercase', letterSpacing: 1.2, color: T.fgDim, marginTop: 6 }}>XMP sidecars</div>
        <div style={{ background: T.hover, borderRadius: 3, padding: 10, fontFamily: 'JetBrains Mono, monospace', fontSize: 10, color: T.fgDim, lineHeight: 1.5 }}>
          <div style={{ color: T.fg }}>318 files · ratings + labels</div>
          <div>written beside each .NEF</div>
          <div>last sync · just now</div>
        </div>
        <button style={{ ...T.btnAccent, marginTop: 'auto' }}>Export XMP sidecars</button>
      </div>
    </div>
  );
}

function DestCard({ name, sub, kbd, count }) {
  const T = darkTheme;
  return (
    <div style={{ padding: 10, borderRadius: 4, border: T.border, background: T.hover }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 3 }}>
        <div style={{ fontSize: 12, color: T.fg, fontWeight: 500 }}>{name}</div>
        <div style={{ display: 'flex', gap: 3 }}>{kbd.split('').map((k, i) => <Kbd key={i}>{k}</Kbd>)}</div>
      </div>
      <div style={{ fontSize: 10, color: T.fgDim }}>{sub}</div>
      <div style={{ fontSize: 9, color: T.accent, fontFamily: 'JetBrains Mono, monospace', marginTop: 3 }}>{count} routed</div>
    </div>
  );
}

window.RouteScreenV2 = RouteScreenV2;
