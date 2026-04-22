// Triage screen v2 — Narrative-Select style
// Layout: left filmstrip (all photos) + group strip + main photo + right face panel
// Filmstrip is toggleable. Not all photos are in groups.

function TriageScreenV2({ showAllStrip = true, showFaces = true }) {
  const T = darkTheme;
  // Mixed feed — some grouped (bursts), some singletons
  const allPhotos = [
    { seed: 'bw-01', group: null, verdict: 'keep', stars: 1 },
    { seed: 'bw-02', group: null, verdict: 'keep' },
    { seed: 'bw-03', group: null, verdict: 'toss' },
    { seed: 'bw-g1-a', group: 'g1', verdict: 'keep' },
    { seed: 'bw-g1-b', group: 'g1', verdict: 'toss' },
    { seed: 'bw-g1-c', group: 'g1', verdict: 'toss' },
    { seed: 'bw-04', group: null },
    { seed: 'bw-05', group: null, verdict: 'keep' },
    { seed: 'bw-g2-a', group: 'g2', verdict: 'keep', current: true },
    { seed: 'bw-g2-b', group: 'g2' },
    { seed: 'bw-g2-c', group: 'g2' },
    { seed: 'bw-g2-d', group: 'g2' },
    { seed: 'bw-06', group: null },
    { seed: 'bw-07', group: null },
    { seed: 'bw-g3-a', group: 'g3' },
    { seed: 'bw-g3-b', group: 'g3' },
    { seed: 'bw-08', group: null },
    { seed: 'bw-09', group: null },
  ];

  const currentGroup = allPhotos.filter(p => p.group === 'g2');
  const faces = [
    { id: 1, verdict: 'keep', conf: 94 },
    { id: 2, verdict: 'keep', conf: 88 },
    { id: 3, verdict: 'blink', conf: 71 },
    { id: 4, verdict: 'keep', conf: 91 },
    { id: 5, verdict: 'blur', conf: 54 },
    { id: 6, verdict: 'keep', conf: 86 },
    { id: 7, verdict: 'keep', conf: 82 },
  ];

  return (
    <div style={{ ...T.root, display: 'grid', gridTemplateColumns: `${showAllStrip ? '92px ' : ''}148px 1fr ${showFaces ? '220px' : ''}`, height: '100%' }}>
      {/* All-photos filmstrip */}
      {showAllStrip && (
        <div style={{ background: T.rail.background, borderRight: T.border, overflow: 'auto', padding: '8px 6px' }}>
          {allPhotos.map((p, i) => (
            <FilmstripThumb key={i} photo={p} active={p.current} compact />
          ))}
        </div>
      )}

      {/* Current group strip */}
      <div style={{ background: T.bg2, borderRight: T.border, overflow: 'auto', padding: '10px 10px', display: 'flex', flexDirection: 'column', gap: 8 }}>
        <div style={{ fontSize: 9, textTransform: 'uppercase', letterSpacing: 1.2, color: T.fgDim, padding: '0 2px', display: 'flex', justifyContent: 'space-between' }}>
          <span>Group G2</span>
          <span style={{ fontFamily: 'JetBrains Mono, monospace' }}>4 · 0.5s</span>
        </div>
        {currentGroup.map((p, i) => (
          <FilmstripThumb key={i} photo={p} active={p.current} group />
        ))}
      </div>

      {/* Main photo area */}
      <div style={{ display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        {/* Top bar */}
        <div style={{ padding: '10px 16px', borderBottom: T.border, display: 'flex', alignItems: 'center', gap: 16, fontSize: 11 }}>
          <span style={{ color: T.fg, fontFamily: 'JetBrains Mono, monospace', fontWeight: 500 }}>DSC_0418.NEF</span>
          <span style={{ color: T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>1/800 · f/2.8 · ISO 400 · 85mm</span>
          <div style={{ flex: 1 }} />
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <span style={{ color: T.fgDim }}>Toggle strip</span>
            <Kbd>T</Kbd>
            <span style={{ color: T.fgDim, marginLeft: 10 }}>Faces</span>
            <Kbd>F</Kbd>
          </div>
          <div style={{ color: T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>42 / 318</div>
        </div>

        {/* Photo */}
        <div style={{ flex: 1, position: 'relative', display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 24, minHeight: 0, background: '#0c0c0c' }}>
          <div style={{ position: 'relative', maxWidth: '100%', maxHeight: '100%', width: '100%', height: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <Photo seed="bw-g2-a" w="100%" h="100%" style={{ maxWidth: 780, maxHeight: '100%' }} />
          </div>

          {/* Verdict overlays — left/right subtle hints */}
          <div style={{ position: 'absolute', left: 24, top: '50%', transform: 'translateY(-50%)', display: 'flex', flexDirection: 'column', gap: 10, alignItems: 'center' }}>
            <div style={{ padding: '10px 12px', background: 'rgba(111,187,123,0.12)', border: `1px solid ${T.success}`, borderRadius: 4, display: 'flex', alignItems: 'center', gap: 10 }}>
              <Kbd>P</Kbd>
              <div style={{ fontSize: 12, color: T.success, fontWeight: 500 }}>Keep</div>
            </div>
          </div>
          <div style={{ position: 'absolute', right: 24, top: '50%', transform: 'translateY(-50%)' }}>
            <div style={{ padding: '10px 12px', background: 'rgba(217,122,122,0.08)', border: `1px solid rgba(217,122,122,0.4)`, borderRadius: 4, display: 'flex', alignItems: 'center', gap: 10 }}>
              <div style={{ fontSize: 12, color: T.danger, fontWeight: 500 }}>Toss</div>
              <Kbd>X</Kbd>
            </div>
          </div>
        </div>

        {/* Bottom bar */}
        <div style={{ padding: '8px 16px', borderTop: T.border, background: T.bg2, display: 'flex', alignItems: 'center', gap: 16, fontSize: 11, color: T.fgDim }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>P</Kbd> keep</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>X</Kbd> toss</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>␣</Kbd> skip</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>⇧</Kbd><Kbd>P</Kbd> keep all in group</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}><Kbd>Z</Kbd> undo</div>
          <div style={{ flex: 1 }} />
          <span style={{ fontFamily: 'JetBrains Mono, monospace' }}>
            <span style={{ color: T.success }}>● 5 kept</span>
            <span style={{ margin: '0 10px', color: T.danger }}>● 3 tossed</span>
            <span>276 left</span>
          </span>
        </div>
      </div>

      {/* Face panel */}
      {showFaces && (
        <div style={{ background: T.rail.background, borderLeft: T.border, overflow: 'auto', padding: '12px 12px' }}>
          <div style={{ fontSize: 9, textTransform: 'uppercase', letterSpacing: 1.2, color: T.fgDim, marginBottom: 8 }}>Faces · 7 detected</div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 6 }}>
            {faces.map(f => <FaceChip key={f.id} face={f} />)}
          </div>
          <div style={{ marginTop: 14, paddingTop: 12, borderTop: T.border }}>
            <div style={{ fontSize: 9, textTransform: 'uppercase', letterSpacing: 1.2, color: T.fgDim, marginBottom: 8 }}>Frame scores</div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              <ScoreBar label="sharp" value={92} color={T.accent2} />
              <ScoreBar label="face" value={88} color={T.accent2} />
              <ScoreBar label="eye" value={86} color={T.accent2} />
              <ScoreBar label="smile" value={74} color={T.accent2} />
            </div>
          </div>
          <div style={{ marginTop: 14, paddingTop: 12, borderTop: T.border, fontSize: 10, color: T.fgDim, lineHeight: 1.5 }}>
            <div style={{ color: T.success, marginBottom: 4 }}>● Best in group so far</div>
            <div>+4% eye-sharpness vs G2-B. One blink detected.</div>
          </div>
        </div>
      )}
    </div>
  );
}

function FilmstripThumb({ photo, active, compact, group }) {
  const T = darkTheme;
  const size = compact ? { w: 78, h: 52 } : { w: '100%', h: 82 };
  return (
    <div style={{ position: 'relative', marginBottom: compact ? 4 : 0, cursor: 'pointer' }}>
      <Photo seed={photo.seed} w={size.w} h={size.h} />
      {/* Active outline */}
      {active && (
        <div style={{ position: 'absolute', inset: 0, border: `2px solid ${T.accentBlue}`, pointerEvents: 'none' }} />
      )}
      {/* Verdict badge */}
      {photo.verdict === 'keep' && (
        <div style={{ position: 'absolute', top: 3, right: 3, width: 12, height: 12, background: T.success, borderRadius: 2, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
          <svg width="8" height="8" viewBox="0 0 8 8" fill="none" stroke="#0c0c0c" strokeWidth="1.5"><path d="M1.5 4l1.5 1.5 3.5-3.5" /></svg>
        </div>
      )}
      {photo.verdict === 'toss' && (
        <div style={{ position: 'absolute', top: 3, right: 3, width: 12, height: 12, background: T.danger, borderRadius: 2, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
          <svg width="7" height="7" viewBox="0 0 7 7" fill="none" stroke="#0c0c0c" strokeWidth="1.5"><path d="M1 1l5 5M6 1l-5 5" /></svg>
        </div>
      )}
      {/* Group indicator bar */}
      {photo.group && !group && (
        <div style={{ position: 'absolute', left: 0, top: 0, bottom: 0, width: 2, background: T.accent }} />
      )}
      {/* Stars */}
      {photo.stars > 0 && (
        <div style={{ position: 'absolute', bottom: 2, left: 2, background: 'rgba(0,0,0,0.6)', padding: '1px 3px', borderRadius: 2 }}>
          <Stars n={photo.stars} size={7} />
        </div>
      )}
    </div>
  );
}

function FaceChip({ face }) {
  const T = darkTheme;
  const verdictColor = face.verdict === 'keep' ? T.success : face.verdict === 'blink' ? T.warning : T.danger;
  const verdictLabel = { keep: '✓', blink: '◑', blur: '⌀' }[face.verdict];
  return (
    <div style={{ position: 'relative', aspectRatio: '1', borderRadius: 3, overflow: 'hidden', background: T.bg3 }}>
      <Photo seed={'face-' + face.id} w="100%" h="100%" />
      {/* Verdict corner */}
      <div style={{
        position: 'absolute', bottom: 0, left: 0, right: 0,
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
        padding: '3px 5px',
        background: 'rgba(0,0,0,0.72)',
        fontSize: 9, fontFamily: 'JetBrains Mono, monospace',
      }}>
        <span style={{ color: verdictColor, fontSize: 11 }}>{verdictLabel}</span>
        <span style={{ color: T.fgDim }}>{face.conf}</span>
      </div>
    </div>
  );
}

window.TriageScreenV2 = TriageScreenV2;
