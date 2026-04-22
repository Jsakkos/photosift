// Photosift logo explorations.
const APERTURE_BLADES = 6;

function holePolygon(cx, cy, r, openness, blades = APERTURE_BLADES) {
  const rHole = r * (0.15 + openness * 0.55);
  const step = (Math.PI * 2) / blades;
  const rot = Math.PI / blades - Math.PI / 2;
  const pts = [];
  for (let i = 0; i < blades; i++) {
    const a = i * step + rot;
    pts.push([cx + Math.cos(a) * rHole, cy + Math.sin(a) * rHole]);
  }
  return { pts, rHole };
}

function Aperture({ cx, cy, r, openness = 0.55, blades = APERTURE_BLADES, color = 'currentColor', stroke = 1.3, accent }) {
  const { pts } = holePolygon(cx, cy, r, openness, blades);
  const holePath = 'M' + pts.map(p => p.map(v => v.toFixed(2)).join(' ')).join(' L') + ' Z';
  const step = (Math.PI * 2) / blades;
  const rot = Math.PI / blades - Math.PI / 2;
  const seams = [];
  for (let i = 0; i < blades; i++) {
    const vi = pts[i];
    const a = i * step + rot;
    const aEnd = a + step * 0.95;
    const ex = cx + Math.cos(aEnd) * r;
    const ey = cy + Math.sin(aEnd) * r;
    seams.push([vi, [ex, ey]]);
  }
  return (
    <g>
      <circle cx={cx} cy={cy} r={r} fill="none" stroke={color} strokeWidth={stroke * 1.1} />
      {seams.map((s, i) => (
        <line key={i} x1={s[0][0]} y1={s[0][1]} x2={s[1][0]} y2={s[1][1]}
          stroke={color} strokeWidth={stroke * 0.8} opacity="0.55" strokeLinecap="round" />
      ))}
      <path d={holePath} fill={accent || 'none'} fillOpacity={accent ? 0.9 : 0} stroke={color} strokeWidth={stroke * 1.3} strokeLinejoin="round" />
    </g>
  );
}

// Perspective (3/4) aperture — solid filled blades, contrasted edges
function PerspectiveAperture({ cx, cy, rx, ry, openness = 0.5, blades = 6, color = 'currentColor', stroke = 1.4, shadeTop = true }) {
  const rxHole = rx * (0.15 + openness * 0.5);
  const ryHole = ry * (0.15 + openness * 0.5);
  const step = (Math.PI * 2) / blades;
  const rot = Math.PI / blades - Math.PI / 2;

  const holePts = [];
  for (let i = 0; i < blades; i++) {
    const a = i * step + rot;
    holePts.push([cx + Math.cos(a) * rxHole, cy + Math.sin(a) * ryHole]);
  }
  const outerPts = [];
  for (let i = 0; i < blades; i++) {
    const a = i * step + rot;
    const aOut = a + step * 0.92;
    outerPts.push([cx + Math.cos(aOut) * rx, cy + Math.sin(aOut) * ry]);
  }

  const bladePaths = [];
  for (let i = 0; i < blades; i++) {
    const h0 = holePts[i];
    const h1 = holePts[(i - 1 + blades) % blades];
    const o0 = outerPts[i];
    const o1 = outerPts[(i - 1 + blades) % blades];
    const a = i * step + rot + step * 0.5;
    const upFacing = -Math.sin(a);
    bladePaths.push({
      d: `M${h0[0].toFixed(2)} ${h0[1].toFixed(2)} L${o0[0].toFixed(2)} ${o0[1].toFixed(2)} A${rx.toFixed(2)} ${ry.toFixed(2)} 0 0 0 ${o1[0].toFixed(2)} ${o1[1].toFixed(2)} L${h1[0].toFixed(2)} ${h1[1].toFixed(2)} Z`,
      upFacing,
      innerEdge: [h0, h1],
    });
  }

  const holePath = 'M' + holePts.map(p => p.map(v => v.toFixed(2)).join(' ')).join(' L') + ' Z';

  return (
    <g>
      <ellipse cx={cx} cy={cy + ry * 0.15} rx={rx * 0.98} ry={ry * 0.3} fill="#000" opacity="0.35" />
      {bladePaths.map((b, i) => {
        const shade = shadeTop ? 0.45 + ((b.upFacing + 1) / 2) * 0.55 : 1;
        return (
          <path key={i} d={b.d} fill={color} fillOpacity={shade}
            stroke="#000" strokeOpacity="0.85" strokeWidth={stroke * 1.1} strokeLinejoin="miter" strokeLinecap="butt" />
        );
      })}
      {bladePaths.map((b, i) => {
        const [h0] = b.innerEdge;
        const a = i * step + rot;
        const aOut = a + step * 0.92;
        const oOut = [cx + Math.cos(aOut) * rx, cy + Math.sin(aOut) * ry];
        return (
          <line key={'seam'+i} x1={h0[0]} y1={h0[1]} x2={oOut[0]} y2={oOut[1]}
            stroke="#000" strokeOpacity="0.75" strokeWidth={stroke * 0.9} strokeLinecap="round" />
        );
      })}
      {bladePaths.map((b, i) => {
        const upFacing = b.upFacing;
        if (upFacing < 0.1) return null;
        const [h0, h1] = b.innerEdge;
        const op = 0.4 + upFacing * 0.55;
        return (
          <line key={'hl'+i} x1={h0[0]} y1={h0[1]} x2={h1[0]} y2={h1[1]}
            stroke="#fff" strokeOpacity={op} strokeWidth={stroke * 0.9} strokeLinecap="round" />
        );
      })}
      <ellipse cx={cx} cy={cy} rx={rx} ry={ry} fill="none" stroke="#000" strokeOpacity="0.9" strokeWidth={stroke * 1.0} />
      <path d={holePath} fill="none" stroke="#000" strokeOpacity="0.9" strokeWidth={stroke * 1.0} strokeLinejoin="round" />
    </g>
  );
}

// ─── A: Aperture hourglass ───────────────────────────────────
function LogoA({ size = 96, color = 'currentColor' }) {
  const s = size, r = s * 0.2, stroke = s * 0.014;
  return (
    <svg width={s} height={s} viewBox={`0 0 ${s} ${s}`} fill="none">
      <path d={`M${s*0.22} ${s*0.08} L${s*0.78} ${s*0.08} L${s*0.54} ${s*0.5} L${s*0.78} ${s*0.92} L${s*0.22} ${s*0.92} L${s*0.46} ${s*0.5} Z`}
        stroke={color} strokeWidth={stroke} strokeLinejoin="round" opacity="0.22" fill={color} fillOpacity="0.02" />
      <Aperture cx={s*0.5} cy={s*0.3} r={r} openness={0.6} color={color} stroke={stroke} />
      <Aperture cx={s*0.5} cy={s*0.7} r={r} openness={0.6} color={color} stroke={stroke} />
      <circle cx={s*0.5} cy={s*0.48} r={s*0.008} fill={color} />
      <circle cx={s*0.5} cy={s*0.52} r={s*0.008} fill={color} opacity="0.7" />
    </svg>
  );
}

// ─── B: Sieves with INVERTED CONE of sand + thin stream ─────
function LogoB({ size = 96, color = 'currentColor', sandColor }) {
  const s = size, cx = s * 0.5;
  const sand = sandColor || color;
  const stroke = Math.max(1.2, s * 0.013);

  const tilt = 0.42;
  // Push the 3 apertures DOWN a bit to make room for a proper sand cone above.
  const rings = [
    { y: s * 0.44, rx: s * 0.26, open: 0.78 },
    { y: s * 0.64, rx: s * 0.20, open: 0.50 },
    { y: s * 0.82, rx: s * 0.15, open: 0.22 },
  ];

  const topRing = rings[0];
  const topHoleRx = topRing.rx * (0.15 + topRing.open * 0.5);

  // INVERTED CONE — sits ENTIRELY ABOVE the top aperture.
  // Wide at the very top of the logo, funneling down to a tip that just
  // kisses the top edge of the top aperture.
  const coneHalfW = topRing.rx * 1.05;
  const coneTopY  = s * 0.02;                                  // pushed higher — more breathing room for the cone
  const coneTipX  = cx;
  const coneTipY  = topRing.y - topRing.rx * tilt;             // tip at top edge of top iris

  // Top surface dips (concave) — sand has drained toward the center.
  // The top edge is JAGGED — individual grains break the silhouette so
  // it doesn't look like a clean geometric shape.
  const dipDepth = s * 0.05;

  // Build the top edge as a series of short jagged segments (grain-bumpy)
  const topEdgeSegs = 26;
  const topEdge = [];
  for (let i = 0; i <= topEdgeSegs; i++) {
    const t = i / topEdgeSegs;
    const x = cx - coneHalfW + t * coneHalfW * 2;
    // Concave dip profile
    const dip = Math.sin(t * Math.PI) * dipDepth;
    // Random-ish jitter to break the smooth curve
    const jitter = (Math.sin(i * 4.7) * 0.5 + Math.sin(i * 11.3) * 0.3) * s * 0.008;
    const y = coneTopY + dip + jitter;
    topEdge.push([x, y]);
  }
  const sandBody =
    `M ${topEdge[0][0].toFixed(2)} ${topEdge[0][1].toFixed(2)} ` +
    topEdge.slice(1).map(p => `L ${p[0].toFixed(2)} ${p[1].toFixed(2)}`).join(' ') +
    ` L ${coneTipX + s * 0.018} ${coneTipY.toFixed(2)} L ${coneTipX - s * 0.018} ${coneTipY.toFixed(2)} Z`;

  // Shadow body — drawn DARKER along the right side (light from upper-left)
  // This is the same shape but filled with a darker color, clipped to the right half
  const shadowBody =
    `M ${cx} ${topEdge[Math.floor(topEdgeSegs/2)][1].toFixed(2)} ` +
    topEdge.slice(Math.floor(topEdgeSegs/2) + 1).map(p => `L ${p[0].toFixed(2)} ${p[1].toFixed(2)}`).join(' ') +
    ` L ${coneTipX + s * 0.018} ${coneTipY.toFixed(2)} L ${coneTipX} ${coneTipY.toFixed(2)} Z`;

  // Dense grain texture filling the interior — positioned in a V/cone
  // distribution so grains are densely packed at the wide top and sparse
  // toward the tip. Multiple sizes for natural granularity.
  const bodyGrains = [];
  const grainRows = 9;
  for (let row = 0; row < grainRows; row++) {
    const rt = row / (grainRows - 1);              // 0 at top, 1 at tip
    const localHalfW = coneHalfW * (1 - rt) * 0.9;
    const yBase = coneTopY + dipDepth * 0.9 + rt * (coneTipY - coneTopY - dipDepth * 0.9);
    const count = Math.max(2, Math.round(11 * (1 - rt * 0.7)));
    for (let i = 0; i < count; i++) {
      const t = count > 1 ? i / (count - 1) : 0.5;
      const x = cx - localHalfW + t * localHalfW * 2 + Math.sin(i * 3.1 + row) * s * 0.006;
      const y = yBase + Math.sin(i * 2.3 + row * 1.7) * s * 0.006;
      // LARGER, more varied grain sizes so they read clearly
      const rSize = s * (0.009 + ((i * 7 + row * 13) % 6) * 0.002);
      bodyGrains.push({ x, y, r: rSize });
    }
  }

  // Highlight grains on the LEFT side (lit side)
  const litGrains = [];
  for (let i = 0; i < 10; i++) {
    const t = i / 9;
    const x = cx - coneHalfW * 0.9 + t * coneHalfW * 0.7;
    const dip = Math.sin((x - (cx - coneHalfW)) / (coneHalfW * 2) * Math.PI) * dipDepth;
    const y = coneTopY + dip + s * 0.004 + Math.sin(i * 2.1) * s * 0.003;
    litGrains.push({ x, y, r: s * 0.006 });
  }

  // Thin stream of grains from cone tip down through all three apertures
  const streamY0 = coneTipY + s * 0.005;
  const streamY1 = s * 0.96;
  const streamGrains = [];
  const grainCount = 9;
  for (let i = 0; i < grainCount; i++) {
    const t = i / (grainCount - 1);
    const y = streamY0 + t * (streamY1 - streamY0);
    const xJ = Math.sin(i * 1.7) * s * 0.005;
    streamGrains.push({ x: cx + xJ, y, r: s * (0.011 - t * 0.004) });
  }

  // Dust exiting bottom
  const dust = Array.from({ length: 5 }).map((_, i) => {
    const t = i / 4;
    return {
      x: cx + Math.sin(i * 2.3) * s * 0.014,
      y: s * 0.94 + t * s * 0.05,
      r: s * (0.006 - t * 0.003),
      opacity: 1 - t * 0.75,
    };
  });

  return (
    <svg width={s} height={s} viewBox={`0 0 ${s} ${s}`} fill="none">
      {/* Main cone body */}
      <path d={sandBody} fill={sand} stroke="#000" strokeOpacity="0.75" strokeWidth={stroke * 0.9} strokeLinejoin="round" strokeLinecap="round" />
      {/* Shaded right side */}
      <path d={shadowBody} fill="#000" opacity="0.22" />
      {/* Dense grain texture filling body — grains are opaque with dark outlines so they pop */}
      {bodyGrains.map((g, i) => (
        <SandGrain key={'bg-'+i} x={g.x} y={g.y} r={g.r} color={sand} />
      ))}
      {/* Lit grains on the top-left */}
      {litGrains.map((g, i) => (
        <SandGrain key={'lg-'+i} x={g.x} y={g.y} r={g.r} color="#fff" opacity="0.25" />
      ))}
      {/* Shadow line under the dip */}
      <path d={`M ${cx - coneHalfW * 0.75} ${coneTopY + dipDepth * 0.4}
                 Q ${cx} ${coneTopY + dipDepth * 1.25}, ${cx + coneHalfW * 0.75} ${coneTopY + dipDepth * 0.4}`}
        fill="none" stroke="#000" strokeOpacity="0.45" strokeWidth={stroke * 1.4} strokeLinecap="round" />

      {/* Top ring */}
      <PerspectiveAperture cx={cx} cy={rings[0].y} rx={rings[0].rx} ry={rings[0].rx * tilt} openness={rings[0].open} color={color} stroke={stroke} />

      {/* Stream grains through top→middle */}
      {streamGrains.filter(g => g.y < rings[1].y - s * 0.01).map((g, i) => (
        <SandGrain key={'st1-'+i} x={g.x} y={g.y} r={g.r} color={sand} />
      ))}

      {/* Middle ring */}
      <PerspectiveAperture cx={cx} cy={rings[1].y} rx={rings[1].rx} ry={rings[1].rx * tilt} openness={rings[1].open} color={color} stroke={stroke} />

      {/* Stream grains middle→bottom */}
      {streamGrains.filter(g => g.y >= rings[1].y - s * 0.01 && g.y < rings[2].y - s * 0.01).map((g, i) => (
        <SandGrain key={'st2-'+i} x={g.x} y={g.y} r={g.r} color={sand} />
      ))}

      {/* Bottom ring */}
      <PerspectiveAperture cx={cx} cy={rings[2].y} rx={rings[2].rx} ry={rings[2].rx * tilt} openness={rings[2].open} color={color} stroke={stroke} />

      {/* Grains below bottom ring */}
      {streamGrains.filter(g => g.y >= rings[2].y - s * 0.01).map((g, i) => (
        <SandGrain key={'st3-'+i} x={g.x} y={g.y} r={g.r} color={sand} />
      ))}

      {/* Dust */}
      {dust.map((d, i) => (
        <SandGrain key={'d-'+i} x={d.x} y={d.y} r={d.r} color={sand} opacity={d.opacity} />
      ))}
    </svg>
  );
}

function SandGrain({ x, y, r, color, opacity = 1, outline = true }) {
  const rot = ((x * 73 + y * 41) % 360);
  const aspect = 0.7 + ((x * 17 + y * 29) % 100) / 333;
  return (
    <ellipse cx={x} cy={y} rx={r} ry={r * aspect} fill={color} opacity={opacity}
      stroke={outline ? '#000' : 'none'} strokeOpacity="0.55" strokeWidth={r * 0.35}
      transform={`rotate(${rot} ${x} ${y})`} />
  );
}

// ─── C: Hourglass w/ aperture pinch ──────────────────────────
function LogoC({ size = 96, color = 'currentColor' }) {
  const s = size, cx = s * 0.5, cy = s * 0.5, r = s * 0.16;
  const stroke = s * 0.014;
  return (
    <svg width={s} height={s} viewBox={`0 0 ${s} ${s}`} fill="none">
      <path d={`M${s*0.2} ${s*0.1} L${s*0.8} ${s*0.1} L${cx+r*0.95} ${cy-r*0.3} L${cx-r*0.95} ${cy-r*0.3} Z`}
        stroke={color} strokeWidth={stroke} strokeLinejoin="round" fill={color} fillOpacity="0.06" />
      <path d={`M${cx-r*0.95} ${cy+r*0.3} L${cx+r*0.95} ${cy+r*0.3} L${s*0.8} ${s*0.9} L${s*0.2} ${s*0.9} Z`}
        stroke={color} strokeWidth={stroke} strokeLinejoin="round" fill={color} fillOpacity="0.02" />
      <Aperture cx={cx} cy={cy} r={r} openness={0.5} color={color} stroke={stroke} />
      <line x1={s*0.18} y1={s*0.1} x2={s*0.82} y2={s*0.1} stroke={color} strokeWidth={stroke*1.4} strokeLinecap="round" />
      <line x1={s*0.18} y1={s*0.9} x2={s*0.82} y2={s*0.9} stroke={color} strokeWidth={stroke*1.4} strokeLinecap="round" />
      <circle cx={cx-s*0.02} cy={s*0.65} r={s*0.014} fill={color} opacity="0.75" />
      <circle cx={cx+s*0.03} cy={s*0.72} r={s*0.012} fill={color} opacity="0.6" />
      <circle cx={cx} cy={s*0.8} r={s*0.013} fill={color} opacity="0.5" />
    </svg>
  );
}

// ─── D: Concentric apertures ─────────────────────────────────
function LogoD({ size = 96, color = 'currentColor' }) {
  const s = size, cx = s * 0.5, cy = s * 0.5;
  const stroke = s * 0.012;
  return (
    <svg width={s} height={s} viewBox={`0 0 ${s} ${s}`} fill="none">
      <Aperture cx={cx} cy={cy} r={s*0.42} openness={0.9} color={color} stroke={stroke} />
      <Aperture cx={cx} cy={cy} r={s*0.3}  openness={0.55} color={color} stroke={stroke} />
      <Aperture cx={cx} cy={cy} r={s*0.18} openness={0.22} color={color} stroke={stroke} />
      <circle cx={cx} cy={cy} r={s*0.025} fill={color} />
    </svg>
  );
}

Object.assign(window, { LogoA, LogoB, LogoC, LogoD, Aperture, holePolygon });
