import type { ReactNode } from "react";

type KbdProps = {
  children: ReactNode;
  className?: string;
};

export function Kbd({ children, className }: KbdProps) {
  return (
    <kbd
      className={[
        "inline-flex items-center justify-center",
        "min-w-[18px] h-[18px] px-[5px]",
        "font-mono text-[10px] font-medium",
        "text-[rgba(230,225,218,0.9)]",
        "bg-[rgba(255,255,255,0.06)]",
        "border border-[rgba(255,255,255,0.1)]",
        "rounded-sm",
        "shadow-[inset_0_-1px_0_rgba(0,0,0,0.3)]",
        className ?? "",
      ]
        .join(" ")
        .trim()}
    >
      {children}
    </kbd>
  );
}
