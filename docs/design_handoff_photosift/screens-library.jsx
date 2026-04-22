// Library screen — list of shoots. Matches the MVP's entry point.
function LibraryScreen({ showImportModal = false, importStage = 'form' }) {
  const T = darkTheme;
  const shoots = [
    { title: 'Jessie-Pete', date: '2026-04-08', count: 127, reviewed: 89, stars: 12, kept: 34, tossed: 43, seed: 'jp' },
    { title: 'Bend Weekend', date: '2026-04-02', count: 318, reviewed: 318, stars: 28, kept: 87, tossed: 203, seed: 'bw', done: true },
    { title: 'Audrey Photoshoot', date: '2026-03-28', count: 176, reviewed: 176, stars: 14, kept: 42, tossed: 120, seed: 'ap', done: true },
    { title: 'Spring Show', date: '2026-04-18', count: 64, reviewed: 0, stars: 0, kept: 0, tossed: 0, seed: 'ss', importing: true },
  ];
  const archived = [
    { title: 'Crater Lake', date: '2026-02-14', count: 214, stars: 22, seed: 'cl' },
    { title: 'Emma · birthday', date: '2026-01-22', count: 88, stars: 9, seed: 'em' },
    { title: 'Holiday Portraits', date: '2025-12-19', count: 302, stars: 31, seed: 'hp' },
    { title: 'Olympic Coast', date: '2025-11-03', count: 446, stars: 38, seed: 'oc' },
    { title: 'Wedding · B+J', date: '2025-10-11', count: 892, stars: 64, seed: 'wb' },
    { title: 'Park bloc party', date: '2025-09-07', count: 134, stars: 11, seed: 'pp' },
  ];
  return (
    <div style={{ ...T.root, padding: '24px 32px', overflow: 'auto' }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 24 }}>
        <div>
          <div style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: 1.4, color: T.fgDim }}>Library</div>
          <div style={{ fontSize: 22, fontWeight: 600, letterSpacing: -0.4, color: T.fg, marginTop: 2 }}>4 shoots · 685 photos</div>
        </div>
        <div style={{ display: 'flex', gap: 10 }}>
          <button style={T.btnGhost}>＋ New shoot</button>
          <button style={T.btnAccent}>＋ Import</button>
        </div>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 20, marginBottom: 28 }}>
        {shoots.map((s, i) => <ShootCard key={i} shoot={s} />)}
      </div>

      {/* Archived — collapsed row tiles */}
      <ArchivedSection shoots={archived} />

      {showImportModal && (
        <div style={{ position: 'absolute', inset: 0, background: 'rgba(0,0,0,0.55)', display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 10 }}>
          <ImportModal stage={importStage} />
        </div>
      )}
    </div>
  );
}

function ArchivedSection({ shoots }) {
  const T = darkTheme;
  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 12, paddingBottom: 8, borderBottom: T.border }}>
        <svg width="10" height="10" viewBox="0 0 10 10" style={{ color: T.fgDim }}>
          <path d="M3 3.5l2 2 2-2" stroke="currentColor" strokeWidth="1" fill="none" strokeLinecap="round" />
        </svg>
        <div style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: 1.4, color: T.fgDim, fontWeight: 500 }}>Archived</div>
        <div style={{ fontSize: 10, color: T.fgMute, fontFamily: 'JetBrains Mono, monospace' }}>{shoots.length} shoots · 2,076 photos</div>
        <div style={{ flex: 1 }} />
        <button style={{ background: 'none', border: 'none', color: T.fgDim, fontSize: 11, cursor: 'pointer' }}>Show all</button>
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 1, background: T.borderColor }}>
        {shoots.map((s, i) => <ArchivedRow key={i} shoot={s} />)}
      </div>
    </div>
  );
}

function ArchivedRow({ shoot }) {
  const T = darkTheme;
  return (
    <div style={{
      display: 'grid',
      gridTemplateColumns: '56px 1fr 120px 140px 80px',
      alignItems: 'center', gap: 14, padding: '8px 12px',
      background: T.bg2, cursor: 'pointer',
    }}>
      <div style={{ width: 56, height: 38, borderRadius: 3, overflow: 'hidden', opacity: 0.7 }}>
        <Photo seed={shoot.seed} w="100%" h="100%" />
      </div>
      <div style={{ fontSize: 12, color: T.fg, fontWeight: 500 }}>{shoot.title}</div>
      <div style={{ fontSize: 10, color: T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>{shoot.date}</div>
      <div style={{ fontSize: 10, color: T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>{shoot.count} photos · ★ {shoot.stars}</div>
      <div style={{ fontSize: 9, fontFamily: 'JetBrains Mono, monospace', color: T.fgMute, textAlign: 'right', textTransform: 'uppercase', letterSpacing: 1 }}>archived</div>
    </div>
  );
}

function ShootCard({ shoot }) {
  const T = darkTheme;
  return (
    <div style={{ background: T.bg2, borderRadius: 6, overflow: 'hidden', border: T.border, cursor: 'pointer' }}>
      <div style={{ position: 'relative', aspectRatio: '4/3' }}>
        <Photo seed={shoot.seed} w="100%" h="100%" />
        {shoot.importing && (
          <div style={{ position: 'absolute', inset: 0, background: 'rgba(0,0,0,0.5)', display: 'flex', alignItems: 'center', justifyContent: 'center', flexDirection: 'column', gap: 10 }}>
            <div style={{ fontSize: 11, color: T.fg, fontFamily: 'JetBrains Mono, monospace' }}>importing · 18 / 64</div>
            <div style={{ width: 140, height: 3, background: 'rgba(255,255,255,0.1)', borderRadius: 2, overflow: 'hidden' }}>
              <div style={{ width: '28%', height: '100%', background: T.accentBlue }} />
            </div>
          </div>
        )}
        {shoot.done && (
          <div style={{ position: 'absolute', top: 8, right: 8, fontSize: 9, fontFamily: 'JetBrains Mono, monospace', color: T.success, background: 'rgba(0,0,0,0.55)', padding: '2px 6px', borderRadius: 3, textTransform: 'uppercase', letterSpacing: 1 }}>✓ routed</div>
        )}
      </div>
      <div style={{ padding: '12px 14px' }}>
        <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', marginBottom: 4 }}>
          <div style={{ fontSize: 14, fontWeight: 600, color: T.fg }}>{shoot.title}</div>
          <div style={{ fontSize: 10, color: T.fgDim, fontFamily: 'JetBrains Mono, monospace' }}>{shoot.date}</div>
        </div>
        <div style={{ fontSize: 11, color: T.fgDim, marginBottom: 10 }}>{shoot.count} photos</div>
        {/* Stats bar */}
        <div style={{ display: 'flex', gap: 10, fontSize: 10, fontFamily: 'JetBrains Mono, monospace' }}>
          <span style={{ color: T.success }}>● {shoot.kept} kept</span>
          <span style={{ color: T.danger }}>● {shoot.tossed} tossed</span>
          <span style={{ color: T.warning }}>★ {shoot.stars}</span>
        </div>
        {/* Progress bar */}
        {!shoot.importing && (
          <div style={{ marginTop: 10, height: 2, background: T.bg3, borderRadius: 1, overflow: 'hidden', display: 'flex' }}>
            <div style={{ width: `${(shoot.kept / shoot.count) * 100}%`, background: T.success }} />
            <div style={{ width: `${(shoot.tossed / shoot.count) * 100}%`, background: T.danger }} />
          </div>
        )}
      </div>
    </div>
  );
}

function ImportModal({ stage }) {
  const T = darkTheme;
  return (
    <div style={{ background: T.bg2, borderRadius: 8, border: T.border, width: 420, padding: 20, boxShadow: '0 20px 80px rgba(0,0,0,0.5)' }}>
      <div style={{ fontSize: 14, fontWeight: 600, color: T.fg, marginBottom: 16 }}>Import Photos</div>
      {stage === 'form' && (
        <>
          <div style={{ fontSize: 11, color: T.fgDim, marginBottom: 6 }}>Import mode</div>
          <label style={{ display: 'flex', gap: 8, padding: '8px 0', cursor: 'pointer' }}>
            <input type="radio" checked readOnly />
            <div>
              <div style={{ fontSize: 12, color: T.fg }}>Copy to library</div>
              <div style={{ fontSize: 10, color: T.fgDim }}>Files are copied into a canonical folder under the library root</div>
            </div>
          </label>
          <label style={{ display: 'flex', gap: 8, padding: '8px 0', cursor: 'pointer' }}>
            <input type="radio" readOnly />
            <div>
              <div style={{ fontSize: 12, color: T.fg }}>Import in-place</div>
              <div style={{ fontSize: 10, color: T.fgDim }}>Register files where they are. XMP sidecars next to the originals on import</div>
            </div>
          </label>
          <div style={{ fontSize: 11, color: T.fgDim, marginTop: 14, marginBottom: 4 }}>Source folder</div>
          <div style={{ display: 'flex', gap: 6 }}>
            <input readOnly value="E:/photos/DSCIM/2026-04-18_Spring-Show" style={{ flex: 1, background: T.bg3, border: T.border, color: T.fg, padding: '6px 10px', borderRadius: 4, fontSize: 11, fontFamily: 'JetBrains Mono, monospace' }} />
            <button style={T.btnGhost}>Browse</button>
          </div>
          <div style={{ fontSize: 11, color: T.fgDim, marginTop: 14, marginBottom: 4 }}>Description</div>
          <input defaultValue="Spring Show" style={{ width: '100%', background: T.bg3, border: T.border, color: T.fg, padding: '6px 10px', borderRadius: 4, fontSize: 12 }} />
          <label style={{ display: 'flex', gap: 8, marginTop: 14, alignItems: 'center' }}>
            <input type="checkbox" />
            <div style={{ fontSize: 11, color: T.fgDim }}>Select subset · preview each group before adding</div>
          </label>
          <div style={{ marginTop: 14, padding: 10, background: T.bg3, borderRadius: 4, fontSize: 11, color: T.fgDim }}>
            <div style={{ color: T.fg, marginBottom: 2 }}>64 photos ready to import</div>
            <div>2.7 GB · everything under the source folder will be imported</div>
          </div>
          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 18 }}>
            <button style={T.btnGhost}>Cancel</button>
            <button style={T.btnAccent}>Import all 64</button>
          </div>
        </>
      )}
      {stage === 'progress' && (
        <>
          <div style={{ fontSize: 12, color: T.fg, marginBottom: 4 }}>Processing files…</div>
          <div style={{ height: 4, background: T.bg3, borderRadius: 2, overflow: 'hidden', marginTop: 10 }}>
            <div style={{ width: '28%', height: '100%', background: T.accentBlue }} />
          </div>
          <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 10, color: T.fgDim, marginTop: 8, fontFamily: 'JetBrains Mono, monospace' }}>
            <span>18 / 64 · DSC_0426.NEF</span>
            <button style={{ background: 'none', border: 'none', color: T.danger, cursor: 'pointer', fontSize: 11 }}>Cancel import</button>
          </div>
          {/* Pipeline */}
          <div style={{ marginTop: 16, paddingTop: 14, borderTop: T.border, display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 6, fontSize: 10, fontFamily: 'JetBrains Mono, monospace' }}>
            <PipelineRow label="copy RAW" done={18} total={64} running />
            <PipelineRow label="extract preview" done={18} total={64} running />
            <PipelineRow label="read EXIF" done={18} total={64} running />
            <PipelineRow label="p-hash group" done={14} total={64} running />
            <PipelineRow label="sharpness" done={8} total={64} running />
            <PipelineRow label="face · eye · smile" done={5} total={64} running />
          </div>
        </>
      )}
    </div>
  );
}

function PipelineRow({ label, done, total, running }) {
  const T = darkTheme;
  const pct = (done / total) * 100;
  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', color: running ? T.fg : T.fgDim, marginBottom: 3 }}>
        <span>{label}</span>
        <span>{done}/{total}</span>
      </div>
      <div style={{ height: 2, background: T.bg3, borderRadius: 1, overflow: 'hidden' }}>
        <div style={{ width: `${pct}%`, height: '100%', background: running ? T.accent : T.fgMute }} />
      </div>
    </div>
  );
}

window.LibraryScreen = LibraryScreen;
