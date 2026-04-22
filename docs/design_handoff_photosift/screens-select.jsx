// Select screen v2 — parallel to Triage. Drill-down hero view by default;
// grid is a variant.
//
// Structure (drill-down):
//   left filmstrip (all kept photos)   filtered by current ★≥N pass
//   | current-rating strip             photos at the same rating as hero
//   | hero photo + rating overlays
//   | right rail: stars picker · faces · scores · label color

function SelectScreenV2({
  variant = 'hero',          // 'hero' | 'grid'
  showAllStrip = true,
  showRail = true,
  passLevel = 2,             // current pass: ★≥N
}) {
  const T = darkTheme;

  // A kept-photos pool — stars assigned, some currently focused
  const allPhotos = [
    { seed: 's-01', stars: 1 },
    { seed: 's-02', stars: 2 },
    { seed: 's-03', stars: 1 },
    { seed: 's-04', stars: 3 },
    { seed: 's-05', stars: 2 },
    { seed: 's-06', stars: 1 },
    { seed: 's-07', stars: 1 },
    { seed: 's-08', stars: 2, current: true },
    { seed: 's-09', stars: 1 },
    { seed: 's-10', stars: 1 },
    { seed: 's-11', stars: 2 },
    { seed: 's-12', stars: 3 },
    { seed: 's-13', stars: 1 },
    { seed: 's-14', stars: 1 },
    { seed: 's-15', stars: 2 },
    { seed: 's-16', stars: 1 },
    { seed: 's-17', stars: 2 },
    { seed: 's-18', stars: 1 },
    { seed: 's-19', stars: 1 },
    { seed: 's-20', stars: 2 },
    { seed: 's-21', stars: 1 },
    { seed: 's-22', stars: 1 },
    { seed: 's-23', stars: 3 },
    { seed: 's-24', stars: 1 },
  ];

  // In grid variant, fall back to the grid-with-rail layout (original)
  if (variant === 'grid') {
    return <SelectGridVariant photos={allPhotos} T={T} />;
  }

  // Hero (drill-down) variant — mirrors Triage structure
  const current = allPhotos.find(p => p.current) || allPhotos[0];
  // "rating strip" = other photos at the same star level, ordered by seed
  const ratingPeers = allPhotos.filter(p => p.stars === current.stars);

  const faces = [
    { id: 1, verdict: 'keep',  conf: 92 },
    { id: 2, verdict: 'keep',  conf: 88 },
    { id: 3, verdict: 'blink', conf: 71 },
    { id: 4, verdict: 'keep',  conf: 86 },
  ];

  return (
    <div style={{ ...T.root, display: 'grid', gridTemplateColumns: `${showAllStrip ? '92px ' : ''}148px 1fr ${showRail ? '220px' : ''}`, height: '100%' }}>
      {/* All-photos filmstrip — grouped by star level */}
      {showAllStrip && (
        <div style={{ background: T.rail.background, borderRight: T.border, overflow: 'auto', padding: '8px 6px' }}>
          {[3, 2, 1].map(level => {
            const atLevel = allPhotos.filter(p => p.stars === level);
            if (!atLevel.length) return null;
            return (
              <div key={level} style={{ marginBottom: 10 }}>
                <div style={{ fontSize: 9, color: T.fgDim, fontFamily: 'JetBrains Mono, monospace', padding: '2px 2px 6px', display: 'flex', justifyContent: 'space-between' }}>
                  <span>{'★'.repeat(level)}</span>
                  <span style={{ color: T.fgMute }}>{atLevel.length}</span>
                </div>
                {atLevel.map((p, i) => (
                  <SelectStripThumb key={i} photo={p} active={p.current} compact />
                ))}
              </div>
            );
          })}
        </div>
      )}

      {/* Current rating strip */}
      <div style={{ background: T.bg2, borderRight: T.border, overflow: 'auto', padding: '10px 10px', display: 'flex', flexDirection: 'column', gap: 8 }}>
        <div style={{ fontSize: 9, textTransform: 'uppercase', letterSpacing: 1.2, color: T.fgDim, padding: '0 2px', display: 'flex', justifyContent: 'space-between' }}>
          <span>Rating {'★'.repeat(current.stars)}</span>
          <span style={{ fontFamily: 'JetBrains Mono, monospace' }}>{ratingPeers.length}</span>
        </div>
        {ratingPeers.map((p, i) => (
          <SelectStripThumb key={i} photo={p} active={p.current} peer />
        ))}
      </div>

      {/* Main photo area */}
      <div style={{ display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        {/* Top bar — pass indicator + filter pills */}
        <div style={{ padding: '10px 16px', borderBottom: T.border, display: 'flex', alignItems: 'center', gap: 16, fontSize: 11 }}>
          <span style={{ color: T.fg, fontFamily: 'JetBrains Mono, monospace', fontWeight: 500 }}>DSC_0418.NEF</span>
          <span style={{ color: T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>1/800 · f/2.8 · ISO 400 · 85mm</span>
          <div style={{ flex: 1, display: 'flex', justifyContent: 'center' }}>
            <div style={{ display: 'inline-flex', background: T.bg2, padding: 2, borderRadius: 4, gap: 1 }}>
              {['all', '★≥1', '★≥2', '★≥3', '★≥4', '★≥5'].map((n, i) => {
                const active = i === passLevel;
                return (
                  <div key={i} style={{ padding: '4px 10px', fontSize: 10, fontFamily: 'JetBrains Mono, monospace', borderRadius: 3, color: active ? T.bg : T.fgDim, background: active ? T.accent : 'transparent', cursor: 'pointer' }}>{n}</div>
                );
              })}
            </div>
          </div>
          <div style={{ color: T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>Pass {passLevel} · {allPhotos.filter(p => p.stars >= passLevel).length} / {allPhotos.length}</div>
        </div>

        {/* Photo */}
        <div style={{ flex: 1, position: 'relative', display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 24, minHeight: 0, background: '#0c0c0c' }}>
          <div style={{ position: 'relative', maxWidth: '100%', maxHeight: '100%', width: '100%', height: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <Photo seed={current.seed} w="100%" h="100%" style={{ maxWidth: 780, maxHeight: '100%' }} />
          </div>

          {/* Stars overlay (current rating) */}
          <div style={{ position: 'absolute', top: 20, left: 24, display: 'flex', alignItems: 'center', gap: 10, background: 'rgba(0,0,0,0.55)', padding: '6px 10px', borderRadius: 4 }}>
            <Stars n={current.stars} size={14} />
            <span style={{ fontSize: 10, color: T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>current</span>
          </div>

          {/* Rate hint — vertical 1-5 column on the left */}
          <div style={{ position: 'absolute', left: 24, top: '50%', transform: 'translateY(-50%)', display: 'flex', flexDirection: 'column', gap: 6 }}>
            {[1, 2, 3, 4, 5].map(n => {
              const active = n === current.stars;
              return (
                <div key={n} style={{
                  display: 'flex', alignItems: 'center', gap: 10,
                  padding: '6px 10px',
                  background: active ? 'rgba(212,165,116,0.15)' : 'rgba(0,0,0,0.4)',
                  border: active ? `1px solid ${T.accent}` : '1px solid rgba(232,230,226,0.06)',
                  borderRadius: 4,
                }}>
                  <Kbd>{n}</Kbd>
                  <Stars n={n} size={9} />
                </div>
              );
            })}
            <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '6px 10px', background: 'rgba(0,0,0,0.4)', border: '1px solid rgba(232,230,226,0.06)', borderRadius: 4 }}>
              <Kbd>0</Kbd>
              <span style={{ fontSize: 10, color: T.fgDim }}>clear</span>
            </div>
          </div>

          {/* Promote / demote hint — right side */}
          <div style={{ position: 'absolute', right: 24, top: '50%', transform: 'translateY(-50%)', display: 'flex', flexDirection: 'column', gap: 10 }}>
            <div style={{ padding: '10px 12px', background: 'rgba(127,184,217,0.08)', border: `1px solid rgba(127,184,217,0.4)`, borderRadius: 4, display: 'flex', alignItems: 'center', gap: 10 }}>
              <Kbd>Tab</Kbd>
              <div style={{ fontSize: 11, color: T.accent2 }}>Compare</div>
            </div>
            <div style={{ padding: '10px 12px', background: 'rgba(0,0,0,0.4)', border: '1px solid rgba(232,230,226,0.06)', borderRadius: 4, display: 'flex', alignItems: 'center', gap: 10 }}>
              <Kbd>[</Kbd><Kbd>]</Kbd>
              <div style={{ fontSize: 11, color: T.fgDim }}>narrow pass</div>
            </div>
          </div>
        </div>

        {/* Bottom bar */}
        <div style={{ padding: '8px 16px', borderTop: T.border, background: T.bg2, display: 'flex', alignItems: 'center', gap: 16, fontSize: 11, color: T.fgDim }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>1</Kbd>–<Kbd>5</Kbd> rate</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>0</Kbd> clear</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>Tab</Kbd> compare</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>[</Kbd><Kbd>]</Kbd> narrow pass</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>G</Kbd> grid</div>
          <div style={{ flex: 1 }} />
          <span style={{ fontFamily: 'JetBrains Mono, monospace' }}>
            <span style={{ color: T.accent }}>{allPhotos.filter(p => p.stars === 3).length} × ★★★</span>
            <span style={{ margin: '0 10px' }}>{allPhotos.filter(p => p.stars === 2).length} × ★★</span>
            <span>{allPhotos.filter(p => p.stars === 1).length} × ★</span>
          </span>
        </div>
      </div>

      {/* Detail rail */}
      {showRail && (
        <div style={{ background: T.rail.background, borderLeft: T.border, overflow: 'auto', padding: '12px 12px' }}>
          <div style={{ fontSize: 9, textTransform: 'uppercase', letterSpacing: 1.2, color: T.fgDim, marginBottom: 8 }}>Rating</div>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 14 }}>
            <Stars n={current.stars} size={18} />
            <span style={{ fontSize: 10, color: T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>⇧ rated</span>
          </div>

          <div style={{ fontSize: 9, textTransform: 'uppercase', letterSpacing: 1.2, color: T.fgDim, marginBottom: 8 }}>Faces · {faces.length}</div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 6 }}>
            {faces.map(f => <FaceChip key={f.id} face={f} />)}
          </div>

          <div style={{ marginTop: 14, paddingTop: 12, borderTop: T.border }}>
            <div style={{ fontSize: 9, textTransform: 'uppercase', letterSpacing: 1.2, color: T.fgDim, marginBottom: 8 }}>Frame scores</div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              <ScoreBar label="sharp" value={92} color={T.accent2} />
              <ScoreBar label="face"  value={88} color={T.accent2} />
              <ScoreBar label="eye"   value={86} color={T.accent2} />
              <ScoreBar label="smile" value={74} color={T.accent2} />
            </div>
          </div>

          <div style={{ marginTop: 14, paddingTop: 12, borderTop: T.border }}>
            <div style={{ fontSize: 9, textTransform: 'uppercase', letterSpacing: 1.2, color: T.fgDim, marginBottom: 8 }}>Label</div>
            <div style={{ display: 'flex', gap: 4 }}>
              {['#d94a3d','#e8c64a','#4aa96c','#4a82d9','#9c6bd9'].map((c, i) => (
                <div key={c} style={{ width: 18, height: 18, borderRadius: 2, background: c, border: i === 2 ? `1px solid ${T.fg}` : 'none' }} />
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function SelectStripThumb({ photo, active, compact, peer }) {
  const T = darkTheme;
  const size = compact ? { w: 78, h: 52 } : { w: '100%', h: 82 };
  return (
    <div style={{ position: 'relative', marginBottom: compact ? 4 : 0, cursor: 'pointer' }}>
      <Photo seed={photo.seed} w={size.w} h={size.h} />
      {active && (
        <div style={{ position: 'absolute', inset: 0, border: `2px solid ${T.accentBlue}`, pointerEvents: 'none' }} />
      )}
      {photo.stars > 0 && (
        <div style={{ position: 'absolute', bottom: 2, left: 2, background: 'rgba(0,0,0,0.65)', padding: '1px 3px', borderRadius: 2 }}>
          <Stars n={photo.stars} size={7} />
        </div>
      )}
    </div>
  );
}

// Grid variant — preserves the earlier 3-col layout (filmstrip + grid + rail)
function SelectGridVariant({ photos, T }) {
  return (
    <div style={{ ...T.root, display: 'grid', gridTemplateColumns: '92px 1fr 260px' }}>
      <div style={{ background: T.rail.background, borderRight: T.border, overflow: 'auto', padding: 6 }}>
        {photos.slice(0, 14).map((p, i) => (
          <div key={i} style={{ position: 'relative', marginBottom: 4 }}>
            <Photo seed={p.seed} w={78} h={52} />
            {p.current && <div style={{ position: 'absolute', inset: 0, border: `2px solid ${T.accentBlue}` }} />}
            <div style={{ position: 'absolute', bottom: 2, left: 2, background: 'rgba(0,0,0,0.6)', padding: '1px 3px', borderRadius: 2 }}>
              <Stars n={p.stars} size={7} />
            </div>
          </div>
        ))}
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        <div style={{ padding: '10px 16px', borderBottom: T.border, display: 'flex', alignItems: 'center', gap: 14, fontSize: 11 }}>
          <div style={{ color: T.fg }}>Pass 2 · showing <span style={{ color: T.accent, fontFamily: 'JetBrains Mono, monospace' }}>★ ≥ 1</span></div>
          <div style={{ flex: 1, display: 'flex', justifyContent: 'center' }}>
            <div style={{ display: 'inline-flex', background: T.bg2, padding: 2, borderRadius: 4, gap: 1 }}>
              {['all', '★≥1', '★≥2', '★≥3', '★≥4', '★≥5'].map((n, i) => (
                <div key={i} style={{ padding: '4px 10px', fontSize: 10, fontFamily: 'JetBrains Mono, monospace', borderRadius: 3, color: i === 1 ? T.bg : T.fgDim, background: i === 1 ? T.accent : 'transparent', cursor: 'pointer' }}>{n}</div>
              ))}
            </div>
          </div>
          <div style={{ color: T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>{photos.length} photos</div>
        </div>

        <div style={{ flex: 1, overflow: 'auto', padding: 14, display: 'grid', gridTemplateColumns: 'repeat(6, 1fr)', gap: 10, alignContent: 'start' }}>
          {photos.map((p, i) => (
            <div key={i} style={{ position: 'relative', outline: p.current ? `2px solid ${T.accentBlue}` : 'none', outlineOffset: 2 }}>
              <Photo seed={p.seed} w="100%" h={110} />
              <div style={{ position: 'absolute', bottom: 4, left: 4, background: 'rgba(0,0,0,0.6)', padding: '2px 5px', borderRadius: 2 }}>
                <Stars n={p.stars} size={9} />
              </div>
            </div>
          ))}
        </div>

        <div style={{ padding: '8px 16px', borderTop: T.border, background: T.bg2, display: 'flex', gap: 16, fontSize: 11, color: T.fgDim }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>1</Kbd>–<Kbd>5</Kbd> rate</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>0</Kbd> clear</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>[</Kbd><Kbd>]</Kbd> narrow</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>Tab</Kbd> compare</div>
          <div style={{ flex: 1 }} />
          <span style={{ fontFamily: 'JetBrains Mono, monospace' }}>14 × ★ · 7 × ★★ · 3 × ★★★</span>
        </div>
      </div>

      <div style={{ borderLeft: T.border, background: T.rail.background, padding: 14, display: 'flex', flexDirection: 'column', gap: 12 }}>
        <Photo seed="s-7" w="100%" h={150} />
        <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 11 }}>
          <span style={{ color: T.fg, fontFamily: 'JetBrains Mono, monospace' }}>DSC_0418</span>
          <Stars n={2} />
        </div>
        <div style={{ fontSize: 9, color: T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>1/800 · f/2.8 · ISO 400 · 85mm</div>
        <div>
          <div style={{ fontSize: 9, textTransform: 'uppercase', letterSpacing: 1.2, color: T.fgDim, marginBottom: 6 }}>Scores</div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
            <ScoreBar label="sharp" value={92} color={T.accent2} />
            <ScoreBar label="face"  value={88} color={T.accent2} />
            <ScoreBar label="eye"   value={86} color={T.accent2} />
          </div>
        </div>
      </div>
    </div>
  );
}

window.SelectScreenV2 = SelectScreenV2;
