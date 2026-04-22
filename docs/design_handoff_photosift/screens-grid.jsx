// Grid mode — dense thumbnail grid with star badges.
// Shared across Library / Triage / Select / Route (G hotkey toggles it).
// Contextual actions in the bottom hint bar change per stage.

function GridMode({
  stage = 'select',
  project = 'Bend Weekend · 318',
  focusedIndex = 11,
  // multi-select range (indices, inclusive)
  selectedRange = [9, 14],
}) {
  const T = darkTheme;

  // ~56 photos worth of data — deterministic stars, keep/toss state, focus
  const N = 56;
  const starsSeq = [0,1,1,2,1,0,3,2,1,2,1,0,4,2,1,3,2,1,2,1,1,3,2,1,1,2,4,1,0,2,1,3,2,1,2,1,5,2,1,1,3,2,1,2,1,0,2,1,3,2,1,1,2,4,1,2];
  const stateSeq = ['',' ','keep','','','toss','keep','','','','keep','','','','','','keep','','','',' ','','','','','toss','','','','','keep','','','','','','keep','','','','','',' ','','','','','','keep','','','','','',''];

  // Stage-specific hint bar
  const hints = {
    library: [
      { keys: ['G'], label: 'grid/list' },
      { keys: ['↵'], label: 'open shoot' },
      { keys: ['⌫'], label: 'archive' },
    ],
    triage: [
      { keys: ['P'], label: 'keep', color: T.success },
      { keys: ['X'], label: 'toss', color: T.danger },
      { keys: ['␣'], label: 'skip' },
      { keys: ['Z'], label: 'undo' },
      { keys: ['G'], label: 'grid/hero' },
    ],
    select: [
      { keys: ['1','–','5'], label: 'rate', inline: true },
      { keys: ['0'], label: 'clear' },
      { keys: ['[', ']'], label: 'narrow', inline: true },
      { keys: ['Tab'], label: 'compare' },
      { keys: ['G'], label: 'grid/detail' },
    ],
    route: [
      { keys: ['↵'], label: 'send to destination' },
      { keys: ['Shift','+','↵'], label: 'send all', inline: true },
      { keys: ['G'], label: 'grid/list' },
    ],
  };

  // Stage-specific top bar
  const topbar = {
    library: { left: 'All shoots', mid: 'grid', right: '12 shoots · 4,812 photos' },
    triage:  { left: `${project}`, mid: 'grid · keep/toss', right: '42 / 318 · 5 kept · 3 tossed' },
    select:  { left: 'Pass 2', mid: 'grid · rate', right: `${N} photos · 8 ★★ · 3 ★★★` },
    route:   { left: 'Selects · ★★+', mid: 'grid · preview', right: '12 picks · ready' },
  }[stage];

  const summaryByStage = {
    library: 'Shoots · newest first',
    triage: 'Untriaged · all groups',
    select: '★★ pass · 56 photos',
    route: 'Final picks · 12',
  };

  return (
    <div style={{ ...T.root, display: 'flex', flexDirection: 'column' }}>
      {/* Top bar */}
      <div style={{ padding: '12px 20px', borderBottom: T.border, display: 'flex', alignItems: 'center', gap: 16, fontSize: 11, flexShrink: 0 }}>
        <span style={{ color: T.fg, fontWeight: 500 }}>{topbar.left}</span>
        <span style={{ color: T.fgMute }}>·</span>
        <span style={{ color: T.fgDim, fontFamily: 'JetBrains Mono, monospace', textTransform: 'uppercase', letterSpacing: 1 }}>{topbar.mid}</span>

        <div style={{ flex: 1, display: 'flex', justifyContent: 'center' }}>
          {stage === 'select' && (
            <div style={{ display: 'inline-flex', background: T.bg2, padding: 2, borderRadius: 4, gap: 1 }}>
              {['all', '★≥1', '★≥2', '★≥3', '★≥4', '★≥5'].map((n, i) => (
                <div key={i} style={{ padding: '4px 10px', fontSize: 10, fontFamily: 'JetBrains Mono, monospace', borderRadius: 3, color: i === 2 ? T.bg : T.fgDim, background: i === 2 ? T.accent : 'transparent' }}>{n}</div>
              ))}
            </div>
          )}
          {stage === 'triage' && (
            <div style={{ display: 'inline-flex', background: T.bg2, padding: 2, borderRadius: 4, gap: 1 }}>
              {['all', 'untriaged', 'kept', 'tossed'].map((n, i) => (
                <div key={i} style={{ padding: '4px 10px', fontSize: 10, fontFamily: 'JetBrains Mono, monospace', borderRadius: 3, color: i === 1 ? T.bg : T.fgDim, background: i === 1 ? T.accent : 'transparent' }}>{n}</div>
              ))}
            </div>
          )}
        </div>

        <span style={{ color: T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>{topbar.right}</span>
      </div>

      {/* Grid */}
      <div style={{
        flex: 1, overflow: 'auto', padding: 18,
        display: 'grid', gridTemplateColumns: 'repeat(8, 1fr)', gap: 10,
        alignContent: 'start', background: T.bg,
      }}>
        {Array.from({ length: N }).map((_, i) => {
          const seed = `grid-${stage}-${i}`;
          const stars = starsSeq[i] || 0;
          const state = stateSeq[i] || '';
          const focused = i === focusedIndex;
          const inRange = i >= selectedRange[0] && i <= selectedRange[1];
          return (
            <GridCell
              key={i}
              seed={seed}
              stars={stars}
              state={state}
              focused={focused}
              selected={inRange}
              stage={stage}
              T={T}
            />
          );
        })}
      </div>

      {/* Bottom hint bar */}
      <div style={{ padding: '10px 20px', borderTop: T.border, background: T.bg2, display: 'flex', alignItems: 'center', gap: 18, fontSize: 11, color: T.fgDim, flexShrink: 0 }}>
        {hints[stage].map((h, i) => (
          <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            {h.keys.map((k, j) =>
              k === '+' || k === '–' ? (
                <span key={j} style={{ color: T.fgMute }}>{k}</span>
              ) : (
                <Kbd key={j}>{k}</Kbd>
              )
            )}
            <span style={{ color: h.color || T.fgDim, marginLeft: 2 }}>{h.label}</span>
          </div>
        ))}
        <div style={{ flex: 1 }} />
        <span style={{ fontFamily: 'JetBrains Mono, monospace', color: T.fgMute }}>
          {summaryByStage[stage]} · focused #{focusedIndex + 1} · range {selectedRange[0] + 1}–{selectedRange[1] + 1}
        </span>
      </div>
    </div>
  );
}

function GridCell({ seed, stars, state, focused, selected, stage, T }) {
  // Outline priorities: focused (blue) > selected (blue tint) > none
  const outline = focused ? `2px solid ${T.accentBlue}`
                : selected ? `1px solid ${T.accentBlue}`
                : 'none';
  const stateTint = state === 'toss' ? 0.35 : 1;

  return (
    <div style={{ position: 'relative', outline, outlineOffset: focused ? 3 : 1 }}>
      <Photo seed={seed} w="100%" h={110} dim={stateTint} />

      {/* Selected range overlay — subtle blue wash */}
      {selected && !focused && (
        <div style={{ position: 'absolute', inset: 0, background: 'rgba(74,130,217,0.12)', pointerEvents: 'none' }} />
      )}

      {/* Keep/Toss corner mark (triage) */}
      {state === 'keep' && (
        <div style={{ position: 'absolute', top: 4, right: 4, width: 14, height: 14, background: T.success, borderRadius: '50%', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 9, fontWeight: 700, color: '#0a0a0a' }}>✓</div>
      )}
      {state === 'toss' && (
        <div style={{ position: 'absolute', top: 4, right: 4, width: 14, height: 14, background: T.danger, borderRadius: '50%', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 9, fontWeight: 700, color: '#0a0a0a' }}>✕</div>
      )}

      {/* Star badge (select + route) */}
      {stars > 0 && (stage === 'select' || stage === 'route') && (
        <div style={{ position: 'absolute', bottom: 4, left: 4, background: 'rgba(0,0,0,0.65)', padding: '2px 5px', borderRadius: 2 }}>
          <Stars n={stars} size={9} />
        </div>
      )}

      {/* Route destination tag */}
      {stage === 'route' && stars >= 2 && (
        <div style={{ position: 'absolute', bottom: 4, right: 4, background: 'rgba(0,0,0,0.65)', padding: '2px 5px', borderRadius: 2, fontSize: 9, fontFamily: 'JetBrains Mono, monospace', color: T.accent2 }}>
          → C1
        </div>
      )}

      {/* Library: shoot-count badge on first 8 cells, otherwise hidden */}
      {stage === 'library' && (
        <div style={{ position: 'absolute', bottom: 4, left: 4, right: 4, display: 'flex', justifyContent: 'space-between', fontSize: 9, fontFamily: 'JetBrains Mono, monospace', color: 'rgba(255,255,255,0.85)', textShadow: '0 1px 2px rgba(0,0,0,0.7)' }}>
          <span>Shoot {seed.slice(-2)}</span>
          <span>{((seed.charCodeAt(seed.length - 1) * 7) % 400) + 40}</span>
        </div>
      )}
    </div>
  );
}

Object.assign(window, { GridMode });
