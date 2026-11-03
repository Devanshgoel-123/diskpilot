import { cn } from "@/lib/utils";

/**
 * The Trash icon used on the Empty Trash button. Drawn like the Lucide set so
 * it sits with the other icons, but with the lid as its own group so it can
 * lift on hover (see `.tg-lid` in trash.css). Place it inside a `.group`.
 */
export function TrashGlyph({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      className={cn("size-3.5 shrink-0 overflow-visible", className)}
    >
      <g className="tg-lid">
        <path d="M3 6h18" />
        <path d="M9 6V4.5A1.5 1.5 0 0 1 10.5 3h3A1.5 1.5 0 0 1 15 4.5V6" />
      </g>
      <path d="M5.2 6.5 6 19a2 2 0 0 0 2 1.9h8a2 2 0 0 0 2-1.9l.8-12.5" />
      <path d="M10 11v5.5M14 11v5.5" />
    </svg>
  );
}
