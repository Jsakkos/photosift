import { useMemo, useState, type ReactElement } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  computeSummary,
  useBenchmarkStore,
  type BenchmarkSummary,
  type FaceDetectionStats,
  type SharpnessSignalAgg,
  type SharpnessVerdictGroup,
} from "../../stores/benchmarkStore";
import { SUBJECT_SHARPNESS_LABEL } from "../../types/benchmark";

interface Props {
  onClose: () => void;
}

function pct(value: number | null): string {
  if (value === null) return "—";
  return `${(value * 100).toFixed(1)}%`;
}

function accuracyPct(correct: number, total: number): string {
  if (total === 0) return "—";
  return `${((correct / total) * 100).toFixed(1)}%`;
}

function signalCell(agg: SharpnessSignalAgg, digits = 1): string {
  if (agg.mean === null) return "—";
  return `${agg.mean.toFixed(digits)} ± ${agg.stdev === null ? "—" : agg.stdev.toFixed(digits)} (n=${agg.count})`;
}

function faceRow(label: string, stats: FaceDetectionStats): ReactElement {
  return (
    <tr>
      <td className="px-2 py-1">{label}</td>
      <td className="px-2 py-1 font-mono">{stats.truePositive}</td>
      <td className="px-2 py-1 font-mono">{stats.falsePositive}</td>
      <td className="px-2 py-1 font-mono">{stats.missed}</td>
      <td className="px-2 py-1 font-mono">{pct(stats.precision)}</td>
      <td className="px-2 py-1 font-mono">{pct(stats.recall)}</td>
      <td className="px-2 py-1 font-mono">{pct(stats.f1)}</td>
    </tr>
  );
}

/// Build the markdown report. Kept here so all stat formatting lives in
/// one place — the Rust side just writes the text to disk.
function buildMarkdown(summary: BenchmarkSummary, slug: string): string {
  const lines: string[] = [];
  lines.push(`# Benchmark — ${summary.setName}`);
  lines.push("");
  lines.push(
    `_Generated ${new Date().toISOString()} · slug: \`${slug}\` · ${summary.judgedPhotos}/${summary.totalPhotos} photos judged_`,
  );
  lines.push("");
  lines.push("## Face detection");
  lines.push("");
  lines.push("| Scope | TP | FP | Missed | Precision | Recall | F1 |");
  lines.push("|---|---:|---:|---:|---:|---:|---:|");
  const o = summary.faceOverall;
  lines.push(
    `| **Overall** | ${o.truePositive} | ${o.falsePositive} | ${o.missed} | ${pct(o.precision)} | ${pct(o.recall)} | ${pct(o.f1)} |`,
  );
  for (const c of summary.facePerCamera) {
    const s = c.stats;
    lines.push(
      `| ${c.cameraModel} | ${s.truePositive} | ${s.falsePositive} | ${s.missed} | ${pct(s.precision)} | ${pct(s.recall)} | ${pct(s.f1)} |`,
    );
  }
  lines.push("");
  lines.push("## Per-face classifiers");
  lines.push("");
  lines.push("| Signal | Correct | Total | Accuracy |");
  lines.push("|---|---:|---:|---:|");
  lines.push(
    `| Left eye open | ${summary.leftEye.correct} | ${summary.leftEye.total} | ${accuracyPct(summary.leftEye.correct, summary.leftEye.total)} |`,
  );
  lines.push(
    `| Right eye open | ${summary.rightEye.correct} | ${summary.rightEye.total} | ${accuracyPct(summary.rightEye.correct, summary.rightEye.total)} |`,
  );
  lines.push(
    `| Smile | ${summary.smile.correct} | ${summary.smile.total} | ${accuracyPct(summary.smile.correct, summary.smile.total)} |`,
  );
  lines.push(
    `| Species | ${summary.species.correct} | ${summary.species.total} | ${accuracyPct(summary.species.correct, summary.species.total)} |`,
  );
  lines.push("");
  lines.push("## Sharpness verdict vs AI signals");
  lines.push("");
  lines.push(
    "_Mean ± stdev of each AI sharpness signal grouped by the photographer's subjective verdict. " +
      "The signal that best separates `subject_sharp`/`subject_blurry` (or `intended_bokeh` vs `all_sharp`) " +
      "is the one most aligned with the photographer's intent._",
  );
  lines.push("");
  lines.push("| Verdict | Count | Global | Max-eye | Mean-eye | Badge 1–10 |");
  lines.push("|---|---:|---|---|---|---|");
  for (const g of summary.sharpnessGroups) {
    lines.push(
      `| ${SUBJECT_SHARPNESS_LABEL[g.verdict]} | ${g.count} | ${signalCell(g.globalScore)} | ${signalCell(g.maxEyeSharpness)} | ${signalCell(g.meanEyeSharpness)} | ${signalCell(g.aiSharpnessBadge1to10, 2)} |`,
    );
  }
  lines.push("");
  return lines.join("\n");
}

export function BenchmarkSummaryView({ onClose }: Props) {
  const currentSet = useBenchmarkStore((s) => s.currentSet);
  const summary = useMemo(
    () => (currentSet ? computeSummary(currentSet) : null),
    [currentSet],
  );
  const [exportPath, setExportPath] = useState<string | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);

  if (!summary || !currentSet) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <p className="text-[12px]" style={{ color: "var(--color-fg-mute)" }}>
          No set loaded.
        </p>
      </div>
    );
  }

  const handleExport = async () => {
    setExporting(true);
    setExportError(null);
    const markdown = buildMarkdown(summary, currentSet.set.slug);
    try {
      const path = await invoke<string>("benchmark_export_markdown", {
        slug: currentSet.set.slug,
        markdown,
      });
      setExportPath(path);
    } catch (e) {
      setExportError(String(e));
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="flex flex-col h-full overflow-hidden" style={{ background: "var(--color-bg)" }}>
      <header
        className="shrink-0 flex items-center justify-between px-4 py-2 border-b"
        style={{ borderColor: "var(--color-border)" }}
      >
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={onClose}
            className="text-[11px] font-mono"
            style={{ color: "var(--color-fg-mute)" }}
          >
            ← Back
          </button>
          <span className="text-[12px] font-medium" style={{ color: "var(--color-fg)" }}>
            {summary.setName} — Summary
          </span>
          <span className="font-mono text-[11px]" style={{ color: "var(--color-fg-mute)" }}>
            {summary.judgedPhotos}/{summary.totalPhotos} judged
          </span>
        </div>
        <div className="flex items-center gap-2">
          {exportPath && (
            <span className="font-mono text-[10px]" style={{ color: "var(--color-fg-mute)" }}>
              wrote {exportPath}
            </span>
          )}
          {exportError && (
            <span className="text-[10px]" style={{ color: "var(--color-danger)" }}>
              {exportError}
            </span>
          )}
          <button
            type="button"
            onClick={handleExport}
            disabled={exporting}
            className="px-3 py-1 rounded-md text-xs font-medium"
            style={{
              background: "var(--color-accent)",
              color: "var(--color-bg)",
            }}
          >
            {exporting ? "Exporting…" : "Export markdown"}
          </button>
        </div>
      </header>

      <main
        className="flex-1 overflow-y-auto p-4 flex flex-col gap-6"
        style={{ color: "var(--color-fg)" }}
      >
        <section>
          <h2 className="text-[13px] font-medium mb-2">Face detection</h2>
          <table
            className="w-full text-[11px] rounded-md overflow-hidden"
            style={{
              background: "var(--color-bg2)",
              border: "1px solid var(--color-border)",
            }}
          >
            <thead style={{ background: "var(--color-bg3)" }}>
              <tr>
                <th className="px-2 py-1 text-left">Scope</th>
                <th className="px-2 py-1 text-right">TP</th>
                <th className="px-2 py-1 text-right">FP</th>
                <th className="px-2 py-1 text-right">Missed</th>
                <th className="px-2 py-1 text-right">Precision</th>
                <th className="px-2 py-1 text-right">Recall</th>
                <th className="px-2 py-1 text-right">F1</th>
              </tr>
            </thead>
            <tbody>
              {faceRow("Overall", summary.faceOverall)}
              {summary.facePerCamera.map((c) => faceRow(c.cameraModel, c.stats))}
            </tbody>
          </table>
        </section>

        <section>
          <h2 className="text-[13px] font-medium mb-2">Per-face classifiers</h2>
          <table
            className="w-full text-[11px] rounded-md overflow-hidden"
            style={{
              background: "var(--color-bg2)",
              border: "1px solid var(--color-border)",
            }}
          >
            <thead style={{ background: "var(--color-bg3)" }}>
              <tr>
                <th className="px-2 py-1 text-left">Signal</th>
                <th className="px-2 py-1 text-right">Correct</th>
                <th className="px-2 py-1 text-right">Total</th>
                <th className="px-2 py-1 text-right">Accuracy</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td className="px-2 py-1">Left eye open</td>
                <td className="px-2 py-1 font-mono">{summary.leftEye.correct}</td>
                <td className="px-2 py-1 font-mono">{summary.leftEye.total}</td>
                <td className="px-2 py-1 font-mono">
                  {accuracyPct(summary.leftEye.correct, summary.leftEye.total)}
                </td>
              </tr>
              <tr>
                <td className="px-2 py-1">Right eye open</td>
                <td className="px-2 py-1 font-mono">{summary.rightEye.correct}</td>
                <td className="px-2 py-1 font-mono">{summary.rightEye.total}</td>
                <td className="px-2 py-1 font-mono">
                  {accuracyPct(summary.rightEye.correct, summary.rightEye.total)}
                </td>
              </tr>
              <tr>
                <td className="px-2 py-1">Smile</td>
                <td className="px-2 py-1 font-mono">{summary.smile.correct}</td>
                <td className="px-2 py-1 font-mono">{summary.smile.total}</td>
                <td className="px-2 py-1 font-mono">
                  {accuracyPct(summary.smile.correct, summary.smile.total)}
                </td>
              </tr>
              <tr>
                <td className="px-2 py-1">Species (human/cat)</td>
                <td className="px-2 py-1 font-mono">{summary.species.correct}</td>
                <td className="px-2 py-1 font-mono">{summary.species.total}</td>
                <td className="px-2 py-1 font-mono">
                  {accuracyPct(summary.species.correct, summary.species.total)}
                </td>
              </tr>
            </tbody>
          </table>
        </section>

        <section>
          <h2 className="text-[13px] font-medium mb-2">Sharpness verdict vs AI signals</h2>
          <p className="text-[11px] mb-2" style={{ color: "var(--color-fg-mute)" }}>
            Mean ± stdev of each AI sharpness signal, grouped by your subjective verdict.
            Look for the signal whose means cleanly separate <em>subject_sharp</em>/<em>intended_bokeh</em>{" "}
            from <em>subject_blurry</em>/<em>all_blurry</em> — that's the one that tracks your intent best.
          </p>
          <table
            className="w-full text-[11px] rounded-md overflow-hidden"
            style={{
              background: "var(--color-bg2)",
              border: "1px solid var(--color-border)",
            }}
          >
            <thead style={{ background: "var(--color-bg3)" }}>
              <tr>
                <th className="px-2 py-1 text-left">Verdict</th>
                <th className="px-2 py-1 text-right">Count</th>
                <th className="px-2 py-1 text-right">Global score</th>
                <th className="px-2 py-1 text-right">Max-eye</th>
                <th className="px-2 py-1 text-right">Mean-eye</th>
                <th className="px-2 py-1 text-right">Badge 1–10</th>
              </tr>
            </thead>
            <tbody>
              {summary.sharpnessGroups.map((g: SharpnessVerdictGroup) => (
                <tr key={g.verdict}>
                  <td className="px-2 py-1">{SUBJECT_SHARPNESS_LABEL[g.verdict]}</td>
                  <td className="px-2 py-1 font-mono">{g.count}</td>
                  <td className="px-2 py-1 font-mono">{signalCell(g.globalScore)}</td>
                  <td className="px-2 py-1 font-mono">{signalCell(g.maxEyeSharpness)}</td>
                  <td className="px-2 py-1 font-mono">{signalCell(g.meanEyeSharpness)}</td>
                  <td className="px-2 py-1 font-mono">{signalCell(g.aiSharpnessBadge1to10, 2)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      </main>
    </div>
  );
}
