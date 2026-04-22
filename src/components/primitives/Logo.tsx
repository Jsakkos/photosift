import { memo } from "react";

const APERTURE_BLADES = 6;

type AperturePathProps = {
  cx: number;
  cy: number;
  rx: number;
  ry: number;
  openness: number;
  color: string;
  stroke: number;
};

function holePoints(cx: number, cy: number, rx: number, ry: number, openness: number) {
  const rxHole = rx * (0.15 + openness * 0.5);
  const ryHole = ry * (0.15 + openness * 0.5);
  const step = (Math.PI * 2) / APERTURE_BLADES;
  const rot = Math.PI / APERTURE_BLADES - Math.PI / 2;
  const pts: [number, number][] = [];
  for (let i = 0; i < APERTURE_BLADES; i++) {
    const a = i * step + rot;
    pts.push([cx + Math.cos(a) * rxHole, cy + Math.sin(a) * ryHole]);
  }
  return pts;
}

function outerPoints(cx: number, cy: number, rx: number, ry: number) {
  const step = (Math.PI * 2) / APERTURE_BLADES;
  const rot = Math.PI / APERTURE_BLADES - Math.PI / 2;
  const pts: [number, number][] = [];
  for (let i = 0; i < APERTURE_BLADES; i++) {
    const a = i * step + rot;
    const aOut = a + step * 0.92;
    pts.push([cx + Math.cos(aOut) * rx, cy + Math.sin(aOut) * ry]);
  }
  return pts;
}

function PerspectiveAperture({ cx, cy, rx, ry, openness, color, stroke }: AperturePathProps) {
  const holePts = holePoints(cx, cy, rx, ry, openness);
  const outerPts = outerPoints(cx, cy, rx, ry);

  const step = (Math.PI * 2) / APERTURE_BLADES;
  const rot = Math.PI / APERTURE_BLADES - Math.PI / 2;

  const bladePaths = holePts.map((h0, i) => {
    const h1 = holePts[(i - 1 + APERTURE_BLADES) % APERTURE_BLADES];
    const o0 = outerPts[i];
    const o1 = outerPts[(i - 1 + APERTURE_BLADES) % APERTURE_BLADES];
    const a = i * step + rot + step * 0.5;
    const upFacing = -Math.sin(a);
    const d = `M${h0[0].toFixed(2)} ${h0[1].toFixed(2)} L${o0[0].toFixed(2)} ${o0[1].toFixed(2)} A${rx.toFixed(2)} ${ry.toFixed(2)} 0 0 0 ${o1[0].toFixed(2)} ${o1[1].toFixed(2)} L${h1[0].toFixed(2)} ${h1[1].toFixed(2)} Z`;
    return { d, upFacing, h0, h1 };
  });

  const holePath =
    "M" + holePts.map((p) => p.map((v) => v.toFixed(2)).join(" ")).join(" L") + " Z";

  return (
    <g>
      <ellipse cx={cx} cy={cy + ry * 0.15} rx={rx * 0.98} ry={ry * 0.3} fill="#000" opacity="0.35" />
      {bladePaths.map((b, i) => {
        const shade = 0.45 + ((b.upFacing + 1) / 2) * 0.55;
        return (
          <path
            key={i}
            d={b.d}
            fill={color}
            fillOpacity={shade}
            stroke="#000"
            strokeOpacity="0.85"
            strokeWidth={stroke * 1.1}
            strokeLinejoin="miter"
          />
        );
      })}
      {bladePaths.map((b, i) => {
        if (b.upFacing < 0.1) return null;
        const op = 0.4 + b.upFacing * 0.55;
        return (
          <line
            key={`hl-${i}`}
            x1={b.h0[0]}
            y1={b.h0[1]}
            x2={b.h1[0]}
            y2={b.h1[1]}
            stroke="#fff"
            strokeOpacity={op}
            strokeWidth={stroke * 0.9}
            strokeLinecap="round"
          />
        );
      })}
      <ellipse
        cx={cx}
        cy={cy}
        rx={rx}
        ry={ry}
        fill="none"
        stroke="#000"
        strokeOpacity="0.9"
        strokeWidth={stroke}
      />
      <path
        d={holePath}
        fill="none"
        stroke="#000"
        strokeOpacity="0.9"
        strokeWidth={stroke}
        strokeLinejoin="round"
      />
    </g>
  );
}

function SandGrain({
  x,
  y,
  r,
  color,
  opacity = 1,
  outline = true,
}: {
  x: number;
  y: number;
  r: number;
  color: string;
  opacity?: number;
  outline?: boolean;
}) {
  const rot = (x * 73 + y * 41) % 360;
  const aspect = 0.7 + ((x * 17 + y * 29) % 100) / 333;
  return (
    <ellipse
      cx={x}
      cy={y}
      rx={r}
      ry={r * aspect}
      fill={color}
      opacity={opacity}
      stroke={outline ? "#000" : "none"}
      strokeOpacity="0.55"
      strokeWidth={r * 0.35}
      transform={`rotate(${rot} ${x} ${y})`}
    />
  );
}

type LogoBProps = {
  size?: number;
  color?: string;
  sandColor?: string;
  className?: string;
};

function LogoBInner({ size = 96, color = "currentColor", sandColor, className }: LogoBProps) {
  const s = size;
  const cx = s * 0.5;
  const sand = sandColor ?? color;
  const stroke = Math.max(1.2, s * 0.013);
  const tilt = 0.42;

  const rings = [
    { y: s * 0.44, rx: s * 0.26, open: 0.78 },
    { y: s * 0.64, rx: s * 0.2, open: 0.5 },
    { y: s * 0.82, rx: s * 0.15, open: 0.22 },
  ];

  const topRing = rings[0];
  const coneHalfW = topRing.rx * 1.05;
  const coneTopY = s * 0.02;
  const coneTipY = topRing.y - topRing.rx * tilt;
  const dipDepth = s * 0.05;

  const topEdgeSegs = 26;
  const topEdge: [number, number][] = [];
  for (let i = 0; i <= topEdgeSegs; i++) {
    const t = i / topEdgeSegs;
    const x = cx - coneHalfW + t * coneHalfW * 2;
    const dip = Math.sin(t * Math.PI) * dipDepth;
    const jitter = (Math.sin(i * 4.7) * 0.5 + Math.sin(i * 11.3) * 0.3) * s * 0.008;
    topEdge.push([x, coneTopY + dip + jitter]);
  }

  const sandBody =
    `M ${topEdge[0][0].toFixed(2)} ${topEdge[0][1].toFixed(2)} ` +
    topEdge.slice(1).map((p) => `L ${p[0].toFixed(2)} ${p[1].toFixed(2)}`).join(" ") +
    ` L ${(cx + s * 0.018).toFixed(2)} ${coneTipY.toFixed(2)} L ${(cx - s * 0.018).toFixed(2)} ${coneTipY.toFixed(2)} Z`;

  // Stream grains from cone tip through all three apertures
  const streamY0 = coneTipY + s * 0.005;
  const streamY1 = s * 0.96;
  const streamGrains = Array.from({ length: 9 }, (_, i) => {
    const t = i / 8;
    const y = streamY0 + t * (streamY1 - streamY0);
    const xJ = Math.sin(i * 1.7) * s * 0.005;
    return { x: cx + xJ, y, r: s * (0.011 - t * 0.004) };
  });

  return (
    <svg
      width={s}
      height={s}
      viewBox={`0 0 ${s} ${s}`}
      fill="none"
      className={className}
      aria-hidden="true"
    >
      <path
        d={sandBody}
        fill={sand}
        stroke="#000"
        strokeOpacity="0.75"
        strokeWidth={stroke * 0.9}
        strokeLinejoin="round"
      />
      <PerspectiveAperture
        cx={cx}
        cy={rings[0].y}
        rx={rings[0].rx}
        ry={rings[0].rx * tilt}
        openness={rings[0].open}
        color={color}
        stroke={stroke}
      />
      {streamGrains
        .filter((g) => g.y < rings[1].y - s * 0.01)
        .map((g, i) => (
          <SandGrain key={`s1-${i}`} x={g.x} y={g.y} r={g.r} color={sand} />
        ))}
      <PerspectiveAperture
        cx={cx}
        cy={rings[1].y}
        rx={rings[1].rx}
        ry={rings[1].rx * tilt}
        openness={rings[1].open}
        color={color}
        stroke={stroke}
      />
      {streamGrains
        .filter((g) => g.y >= rings[1].y - s * 0.01 && g.y < rings[2].y - s * 0.01)
        .map((g, i) => (
          <SandGrain key={`s2-${i}`} x={g.x} y={g.y} r={g.r} color={sand} />
        ))}
      <PerspectiveAperture
        cx={cx}
        cy={rings[2].y}
        rx={rings[2].rx}
        ry={rings[2].rx * tilt}
        openness={rings[2].open}
        color={color}
        stroke={stroke}
      />
      {streamGrains
        .filter((g) => g.y >= rings[2].y - s * 0.01)
        .map((g, i) => (
          <SandGrain key={`s3-${i}`} x={g.x} y={g.y} r={g.r} color={sand} />
        ))}
    </svg>
  );
}

export const LogoB = memo(LogoBInner);
