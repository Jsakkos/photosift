import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { Face } from "../../types";
import type { BenchmarkFaceJudgment } from "../../types/benchmark";

interface Props {
  faces: Face[];
  judgments: BenchmarkFaceJudgment[];
  selectedFaceIndex: number;
  naturalWidth: number | null;
  naturalHeight: number | null;
  onSelectFace: (index: number) => void;
}

/// Stroke color per detection state. Borrows the existing token palette
/// (green = accept, red = reject, dim = neutral) so the overlay reads
/// the same as flag badges elsewhere in the app.
function strokeFor(detectionCorrect: boolean | null, selected: boolean): string {
  if (selected) return "rgb(59, 130, 246)"; // tailwind blue-500 — selection
  if (detectionCorrect === true) return "rgb(34, 197, 94)"; // green-500
  if (detectionCorrect === false) return "rgb(239, 68, 68)"; // red-500
  return "rgba(255, 255, 255, 0.7)"; // unjudged → translucent white
}

/// Container is `absolute inset-0` over the LoupeView image. Image
/// inside that container is `object-contain`, so we need to compute the
/// letterboxed rect ourselves (CSS `aspect-ratio` would size the SVG to
/// the *container*, not the displayed image). Same approach as
/// HeatmapOverlay.tsx — ResizeObserver + manual contain-fit math.
export function BenchmarkFaceOverlay({
  faces,
  judgments,
  selectedFaceIndex,
  naturalWidth,
  naturalHeight,
  onSelectFace,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [rect, setRect] = useState<{ w: number; h: number } | null>(null);

  const aspect =
    naturalWidth && naturalHeight && naturalHeight > 0
      ? naturalWidth / naturalHeight
      : null;

  useLayoutEffect(() => {
    if (!aspect) return;
    const el = containerRef.current;
    if (!el) return;
    const compute = () => {
      const { width, height } = el.getBoundingClientRect();
      if (width === 0 || height === 0) return;
      const containerAspect = width / height;
      if (containerAspect > aspect) {
        setRect({ w: Math.round(height * aspect), h: Math.round(height) });
      } else {
        setRect({ w: Math.round(width), h: Math.round(width / aspect) });
      }
    };
    compute();
    const ro = new ResizeObserver(compute);
    ro.observe(el);
    return () => ro.disconnect();
  }, [aspect]);

  // Keyboard focus on the selected face for screen-reader visibility.
  const svgRef = useRef<SVGSVGElement>(null);
  useEffect(() => {
    if (selectedFaceIndex < 0) return;
    svgRef.current?.focus({ preventScroll: true });
  }, [selectedFaceIndex]);

  return (
    <div
      ref={containerRef}
      className="absolute inset-0 flex items-center justify-center pointer-events-none z-[6]"
    >
      {rect && faces.length > 0 && (
        <svg
          ref={svgRef}
          width={rect.w}
          height={rect.h}
          viewBox={`0 0 ${rect.w} ${rect.h}`}
          className="pointer-events-auto"
          aria-label={`${faces.length} detected faces`}
          tabIndex={-1}
        >
          {faces.map((face, i) => {
            const x = face.bboxX * rect.w;
            const y = face.bboxY * rect.h;
            const w = face.bboxW * rect.w;
            const h = face.bboxH * rect.h;
            const judgment = judgments.find((j) => j.faceIndex === i);
            const detectionCorrect = judgment?.detectionCorrect ?? null;
            const selected = i === selectedFaceIndex;
            const stroke = strokeFor(detectionCorrect, selected);
            const dasharray =
              detectionCorrect === null && !selected ? "6 4" : undefined;

            return (
              <g key={i}>
                <rect
                  x={x}
                  y={y}
                  width={w}
                  height={h}
                  fill="none"
                  stroke={stroke}
                  strokeWidth={selected ? 3 : 2}
                  strokeDasharray={dasharray}
                  onClick={(e) => {
                    e.stopPropagation();
                    onSelectFace(i);
                  }}
                  style={{ cursor: "pointer" }}
                />
                {/* Eye landmark dots — purely informational */}
                <circle
                  cx={face.leftEyeX * rect.w}
                  cy={face.leftEyeY * rect.h}
                  r={3}
                  fill={stroke}
                />
                <circle
                  cx={face.rightEyeX * rect.w}
                  cy={face.rightEyeY * rect.h}
                  r={3}
                  fill={stroke}
                />
                {/* Index label, top-left of the bbox */}
                <rect
                  x={x}
                  y={y - 18}
                  width={22}
                  height={16}
                  fill={stroke}
                  pointerEvents="none"
                />
                <text
                  x={x + 11}
                  y={y - 6}
                  fontSize={11}
                  fontFamily="ui-monospace, monospace"
                  fontWeight={600}
                  fill="#000"
                  textAnchor="middle"
                  pointerEvents="none"
                >
                  {i + 1}
                </text>
              </g>
            );
          })}
        </svg>
      )}
    </div>
  );
}
