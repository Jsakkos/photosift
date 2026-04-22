import {
  ColorLabelChip,
  ColorLabelRow,
  ExifChip,
  Kbd,
  Photo,
  ScoreBar,
  Stars,
  COLOR_LABEL_ORDER,
} from "../components/primitives";
import { TabBar, type TabId } from "../components/chrome/TabBar";
import { useState } from "react";

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mb-10">
      <h2
        className="font-mono text-[10px] uppercase tracking-[1.2px] mb-4"
        style={{ color: "var(--color-fg-mute)" }}
      >
        {title}
      </h2>
      <div
        className="p-6 rounded-md"
        style={{ background: "var(--color-bg2)", border: "1px solid var(--color-border)" }}
      >
        {children}
      </div>
    </section>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-start gap-6 py-2">
      <div
        className="w-32 shrink-0 font-mono text-[10px] uppercase tracking-[1.2px] pt-1"
        style={{ color: "var(--color-fg-mute)" }}
      >
        {label}
      </div>
      <div className="flex items-center flex-wrap gap-4">{children}</div>
    </div>
  );
}

export function PrimitivesPage() {
  const [activeTab, setActiveTab] = useState<TabId>("triage");
  const [labelValue, setLabelValue] = useState<Parameters<typeof ColorLabelRow>[0]["value"]>("green");

  return (
    <div
      className="h-full flex flex-col overflow-hidden"
      style={{ background: "var(--color-bg)", color: "var(--color-fg)" }}
    >
      <TabBar activeTab={activeTab} onSelect={setActiveTab} projectName="primitives-demo" />
      <div className="flex-1 overflow-auto px-12 py-10">
        <header className="mb-10">
          <h1 className="text-[20px] font-medium mb-1" style={{ color: "var(--color-fg)" }}>
            Primitives
          </h1>
          <p style={{ color: "var(--color-fg-dim)" }}>
            Visual smoke-test for shared primitives defined in{" "}
            <code className="font-mono">src/components/primitives/</code>. Compare against{" "}
            <code className="font-mono">docs/design_handoff_photosift/placeholders.jsx</code>.
          </p>
        </header>

        <Section title="Palette">
          <div className="grid grid-cols-4 gap-3">
            {[
              ["bg", "--color-bg"],
              ["bg2", "--color-bg2"],
              ["bg3", "--color-bg3"],
              ["fg", "--color-fg"],
              ["fg-dim", "--color-fg-dim"],
              ["fg-mute", "--color-fg-mute"],
              ["accent", "--color-accent"],
              ["accent-2", "--color-accent-2"],
              ["accent-blue", "--color-accent-blue"],
              ["success", "--color-success"],
              ["danger", "--color-danger"],
              ["warning", "--color-warning"],
            ].map(([name, varname]) => (
              <div
                key={name}
                className="rounded-md overflow-hidden"
                style={{ border: "1px solid var(--color-border)" }}
              >
                <div className="h-12" style={{ background: `var(${varname})` }} />
                <div className="px-3 py-2">
                  <div className="text-[11px]">{name}</div>
                  <div
                    className="font-mono text-[10px]"
                    style={{ color: "var(--color-fg-mute)" }}
                  >
                    {varname}
                  </div>
                </div>
              </div>
            ))}
          </div>
        </Section>

        <Section title="Typography">
          <Row label="Sans 13px">
            <span>The quick brown fox jumps over the lazy dog</span>
          </Row>
          <Row label="Mono 11px">
            <span className="font-mono text-[11px]">1/500 · f/2.8 · ISO 400 · 85mm</span>
          </Row>
          <Row label="Mono 10px caps">
            <span
              className="font-mono text-[10px] uppercase tracking-[1.2px]"
              style={{ color: "var(--color-fg-dim)" }}
            >
              Unreviewed · Keep · Toss
            </span>
          </Row>
        </Section>

        <Section title="Stars">
          <Row label="0 · 1 · 2 · 3 · 4 · 5">
            {[0, 1, 2, 3, 4, 5].map((v) => (
              <Stars key={v} value={v as 0 | 1 | 2 | 3 | 4 | 5} />
            ))}
          </Row>
          <Row label="size 18">
            <Stars value={4} size={18} />
          </Row>
        </Section>

        <Section title="Kbd">
          <Row label="Keys">
            <Kbd>P</Kbd>
            <Kbd>X</Kbd>
            <Kbd>Space</Kbd>
            <Kbd>⇧ P</Kbd>
            <Kbd>⌘ E</Kbd>
            <Kbd>Z</Kbd>
            <Kbd>Tab</Kbd>
            <Kbd>[</Kbd>
            <Kbd>]</Kbd>
            <Kbd>1</Kbd>
            <Kbd>2</Kbd>
            <Kbd>3</Kbd>
            <Kbd>4</Kbd>
            <Kbd>5</Kbd>
          </Row>
        </Section>

        <Section title="ExifChip">
          <Row label="Full">
            <ExifChip shutter="1/500" fstop={2.8} iso={400} focal="85mm" />
          </Row>
          <Row label="Partial">
            <ExifChip shutter="1/250" fstop={4.0} iso={200} />
          </Row>
        </Section>

        <Section title="ScoreBar">
          <div className="max-w-[260px] space-y-2">
            <ScoreBar label="sharp" value={92} tone="accent-2" />
            <ScoreBar label="face" value={88} tone="accent-2" />
            <ScoreBar label="eye" value={74} tone="warning" />
            <ScoreBar label="smile" value={41} tone="danger" />
            <ScoreBar label="pick" value={96} tone="success" />
          </div>
        </Section>

        <Section title="Color labels">
          <Row label="Swatches">
            {COLOR_LABEL_ORDER.map((c) => (
              <ColorLabelChip key={c} color={c} />
            ))}
          </Row>
          <Row label="Row · interactive">
            <ColorLabelRow value={labelValue} onChange={setLabelValue} />
            <span className="font-mono text-[10px]" style={{ color: "var(--color-fg-dim)" }}>
              value = {labelValue ?? "null"}
            </span>
          </Row>
        </Section>

        <Section title="Photo">
          <Row label="Placeholders">
            <Photo src={null} placeholderSeed="A-01" className="rounded-xs" style={{ width: 160, height: 100 }} />
            <Photo src={null} placeholderSeed="B-02" className="rounded-xs" style={{ width: 160, height: 100 }} />
            <Photo src={null} placeholderSeed="C-03" className="rounded-xs" style={{ width: 160, height: 100 }} />
          </Row>
          <Row label="With overlays">
            <Photo
              src={null}
              placeholderSeed="keep-a"
              verdict="keep"
              rating={3}
              groupMember
              className="rounded-xs"
              style={{ width: 160, height: 100 }}
            />
            <Photo
              src={null}
              placeholderSeed="toss-b"
              verdict="toss"
              dim={0.35}
              className="rounded-xs"
              style={{ width: 160, height: 100 }}
            />
            <Photo
              src={null}
              placeholderSeed="selected-c"
              selected
              rating={5}
              colorLabel="green"
              className="rounded-xs"
              style={{ width: 160, height: 100 }}
            />
          </Row>
          <Row label="Sharp low">
            <Photo
              src={null}
              placeholderSeed="blurry"
              sharp={0.1}
              className="rounded-xs"
              style={{ width: 200, height: 130 }}
            />
          </Row>
        </Section>

        <Section title="TabBar">
          <div style={{ border: "1px solid var(--color-border)" }} className="rounded-md overflow-hidden">
            <TabBar activeTab={activeTab} onSelect={setActiveTab} projectName="2026-04-bend-weekend" />
          </div>
        </Section>
      </div>
    </div>
  );
}
