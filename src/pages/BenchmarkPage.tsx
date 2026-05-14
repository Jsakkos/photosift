import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useBenchmarkStore } from "../stores/benchmarkStore";
import { BenchmarkSetListView } from "../components/benchmark/BenchmarkSetListView";
import { BenchmarkSetBuilderView } from "../components/benchmark/BenchmarkSetBuilderView";
import { BenchmarkEvaluatorView } from "../components/benchmark/BenchmarkEvaluatorView";
import { BenchmarkSummaryView } from "../components/benchmark/BenchmarkSummaryView";

type SubView = "list" | "builder" | "evaluator" | "summary";

/// Top-level dev-only page wired into App.tsx behind `import.meta.env.DEV`.
/// Local state (not a router) switches between the four sub-views since
/// the benchmark surface is small and self-contained.
export function BenchmarkPage() {
  const navigate = useNavigate();
  const [subview, setSubview] = useState<SubView>("list");
  const loadSet = useBenchmarkStore((s) => s.loadSet);
  const closeSet = useBenchmarkStore((s) => s.closeSet);

  return (
    <div
      className="fixed inset-0 flex flex-col"
      style={{
        background: "var(--color-bg)",
        color: "var(--color-fg)",
      }}
    >
      <header
        className="shrink-0 px-3 py-1.5 border-b flex items-center justify-between text-[11px]"
        style={{ borderColor: "var(--color-border)" }}
      >
        <div className="flex items-center gap-2">
          <span style={{ color: "var(--color-fg-mute)" }}>🧪</span>
          <span style={{ color: "var(--color-fg)" }} className="font-medium">
            AI quality evaluation (dev only)
          </span>
        </div>
        <button
          type="button"
          onClick={() => {
            closeSet();
            navigate("/shoots");
          }}
          className="font-mono"
          style={{ color: "var(--color-fg-mute)" }}
        >
          ← Back to shoots
        </button>
      </header>

      <div className="flex-1 overflow-hidden">
        {subview === "list" && (
          <BenchmarkSetListView
            onNew={() => setSubview("builder")}
            onOpen={async (slug) => {
              await loadSet(slug);
              setSubview("evaluator");
            }}
          />
        )}
        {subview === "builder" && (
          <BenchmarkSetBuilderView
            onCancel={() => setSubview("list")}
            onCreated={() => setSubview("evaluator")}
          />
        )}
        {subview === "evaluator" && (
          <BenchmarkEvaluatorView
            onClose={() => {
              closeSet();
              setSubview("list");
            }}
            onOpenSummary={() => setSubview("summary")}
          />
        )}
        {subview === "summary" && (
          <BenchmarkSummaryView onClose={() => setSubview("evaluator")} />
        )}
      </div>
    </div>
  );
}
