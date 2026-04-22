// Placeholder photo tiles. Since we don't have real photos, draw striped/gradient
// cards with EXIF-like metadata overlays, labeled by a seed. Consistent colors
// per seed so the same "photo" looks identical across screens.

function photoHue(seed) {
  // Deterministic hue from seed string
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) | 0;
  return ((h % 360) + 360) % 360;
}

// A photo placeholder — subtly-striped "image" with optional chip overlays.
// Style is monochrome-ish, tinted slightly, so they read as photos without
// pretending to be any real subject.
function Photo({ seed = 'x', w = 240, h = 160, children, style = {}, sharp = 0.85, dim = 1, className = '' }) {
  const hue = photoHue(seed);
  // Two muted tones based on hue; the key is it looks like a real photo
  // at a glance but unmistakable as a placeholder on inspection.
  const c1 = `oklch(${0.32 + (seed.length % 5) * 0.03} 0.02 ${hue})`;
  const c2 = `oklch(${0.48 + (seed.length % 3) * 0.04} 0.03 ${hue + 40})`;
  const c3 = `oklch(${0.22} 0.015 ${hue - 20})`;
  // Sharpness visually encoded via an inner blur/sharpening
  const blur = sharp < 0.5 ? `blur(${(0.5 - sharp) * 3}px)` : 'none';
  return (
    <div className={className} style={{
      position: 'relative',
      width: w === '100%' ? '100%' : w,
      height: h === '100%' ? '100%' : h,
      background: `linear-gradient(135deg, ${c1} 0%, ${c2} 60%, ${c3} 100%)`,
      overflow: 'hidden',
      filter: blur,
      opacity: dim,
      ...style,
    }}>
      {/* Subtle diagonal stripe texture so it reads as "not a real image" */}
      <div style={{
        position: 'absolute', inset: 0,
        backgroundImage: `repeating-linear-gradient(${45 + (hue % 60)}deg, rgba(255,255,255,0.04) 0 2px, transparent 2px 18px)`,
        mixBlendMode: 'overlay',
      }} />
      {/* Seed label in corner, monospace */}
      <div style={{
        position: 'absolute', top: 8, left: 10,
        fontFamily: '"JetBrains Mono", ui-monospace, monospace',
        fontSize: 10, color: 'rgba(255,255,255,0.55)', letterSpacing: 0.5,
      }}>{seed}</div>
      {children}
    </div>
  );
}

// A star-rating cluster (outlined by default; filled when set)
function Stars({ n = 0, max = 5, size = 11, color = '#e8d37a' }) {
  return (
    <div style={{ display: 'inline-flex', gap: 1.5 }}>
      {Array.from({ length: max }).map((_, i) => (
        <svg key={i} width={size} height={size} viewBox="0 0 16 16" fill={i < n ? color : 'none'} stroke={i < n ? color : 'rgba(255,255,255,0.35)'} strokeWidth="1.2">
          <path d="M8 1.5l1.95 4.17 4.55.46-3.42 3.12.97 4.5L8 11.45l-4.05 2.3.97-4.5L1.5 6.13l4.55-.46z" strokeLinejoin="round" />
        </svg>
      ))}
    </div>
  );
}

// Color label chip (Lightroom-style)
function ColorLabel({ color }) {
  const map = {
    red: '#d94a3d', yellow: '#e8c64a', green: '#4aa96c', blue: '#4a82d9', purple: '#9c6bd9',
  };
  return <div style={{ width: 8, height: 8, borderRadius: 2, background: map[color] || color }} />;
}

// A compact EXIF chip overlay (shutter/iso/fstop)
function ExifChip({ shutter = '1/500', iso = 400, fstop = 2.8, lens }) {
  return (
    <div style={{
      display: 'inline-flex', alignItems: 'center', gap: 8,
      fontFamily: '"JetBrains Mono", ui-monospace, monospace',
      fontSize: 10, color: 'rgba(255,255,255,0.8)',
      background: 'rgba(0,0,0,0.55)', backdropFilter: 'blur(6px)',
      padding: '3px 7px', borderRadius: 2,
    }}>
      <span>{shutter}</span>
      <span style={{ opacity: 0.4 }}>·</span>
      <span>f/{fstop}</span>
      <span style={{ opacity: 0.4 }}>·</span>
      <span>ISO {iso}</span>
      {lens && <><span style={{ opacity: 0.4 }}>·</span><span>{lens}</span></>}
    </div>
  );
}

// Keyboard shortcut key
function Kbd({ children, dark = true }) {
  return (
    <kbd style={{
      display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
      minWidth: 18, height: 18, padding: '0 5px',
      fontFamily: '"JetBrains Mono", ui-monospace, monospace',
      fontSize: 10, fontWeight: 500,
      color: dark ? 'rgba(230,225,218,0.9)' : 'rgba(40,36,30,0.85)',
      background: dark ? 'rgba(255,255,255,0.06)' : 'rgba(0,0,0,0.05)',
      border: dark ? '1px solid rgba(255,255,255,0.1)' : '1px solid rgba(0,0,0,0.1)',
      borderRadius: 3,
      boxShadow: dark ? 'inset 0 -1px 0 rgba(0,0,0,0.3)' : 'inset 0 -1px 0 rgba(0,0,0,0.08)',
    }}>{children}</kbd>
  );
}

// Score bar — used for AI annotations (sharpness, face, eye, smile)
function ScoreBar({ label, value, color = '#7fb8d9', max = 100 }) {
  const pct = Math.min(100, (value / max) * 100);
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 10, fontFamily: '"JetBrains Mono", ui-monospace, monospace' }}>
      <span style={{ width: 48, color: 'rgba(230,225,218,0.55)', letterSpacing: 0.5, textTransform: 'uppercase' }}>{label}</span>
      <div style={{ flex: 1, height: 3, background: 'rgba(255,255,255,0.08)', borderRadius: 1, overflow: 'hidden' }}>
        <div style={{ width: `${pct}%`, height: '100%', background: color }} />
      </div>
      <span style={{ width: 24, textAlign: 'right', color: 'rgba(230,225,218,0.7)', fontVariantNumeric: 'tabular-nums' }}>{value}</span>
    </div>
  );
}

Object.assign(window, { Photo, Stars, ColorLabel, ExifChip, Kbd, ScoreBar, photoHue });
