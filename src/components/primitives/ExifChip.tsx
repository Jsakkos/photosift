type ExifChipProps = {
  shutter?: string | null;
  fstop?: number | string | null;
  iso?: number | null;
  focal?: string | null;
  className?: string;
};

function Sep() {
  return <span className="opacity-40">·</span>;
}

export function ExifChip({ shutter, fstop, iso, focal, className }: ExifChipProps) {
  const parts: Array<[string, string]> = [];
  if (shutter) parts.push(["shutter", shutter]);
  if (fstop !== null && fstop !== undefined) parts.push(["fstop", `f/${fstop}`]);
  if (iso !== null && iso !== undefined) parts.push(["iso", `ISO ${iso}`]);
  if (focal) parts.push(["focal", focal]);

  if (parts.length === 0) return null;

  return (
    <div
      className={[
        "inline-flex items-center gap-2",
        "font-mono text-[10px] text-[rgba(255,255,255,0.8)]",
        "bg-black/55 backdrop-blur-sm",
        "px-[7px] py-[3px] rounded-xs",
        className ?? "",
      ]
        .join(" ")
        .trim()}
    >
      {parts.map(([key, text], i) => (
        <span key={key} className="inline-flex items-center gap-2">
          {i > 0 && <Sep />}
          <span>{text}</span>
        </span>
      ))}
    </div>
  );
}
