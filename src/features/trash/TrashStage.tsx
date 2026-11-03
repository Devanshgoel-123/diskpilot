import type { CSSProperties } from "react";

import { formatBytes } from "@/lib/format";
import type { TrashEntry } from "@/lib/types";

import { entryIcon } from "./fileKind";
import { STAGE_CHIPS } from "./trashStore";

/** Where the five largest entries rest above the bin: x, y, rotation. */
const SLOTS: [number, number, number][] = [
  [0, 16, -2],
  [-68, 34, -8],
  [68, 34, 7],
  [-128, 66, -13],
  [128, 66, 12],
];

export type StageMood = "confirm" | "working" | "done" | "attention" | "locked" | "empty";

const TONE: Record<StageMood, string> = {
  confirm: "var(--critical)",
  working: "var(--accent)",
  done: "var(--good)",
  attention: "var(--warning)",
  locked: "var(--warning)",
  empty: "var(--muted-foreground)",
};

/** Deterministic drift for the motes of one landing. */
function motes(seed: number) {
  return Array.from({ length: 5 }, (_, i) => {
    const a = ((seed * 47 + i * 73) % 100) / 100;
    return { dx: `${Math.round((a - 0.5) * 70)}px`, dy: `${-28 - Math.round(a * 34) - i * 4}px` };
  });
}

/**
 * The illustrated stage at the top of the Empty Trash window: a bin, and the
 * largest real Trash entries as small cards. During emptying, a card drops
 * into the bin only once the engine reports that entry removed; entries that
 * could not be removed stay out, dimmed.
 */
export function TrashStage({
  mood,
  entries,
  fallen,
  notRemoved,
  lidOpen,
}: {
  mood: StageMood;
  entries: TrashEntry[];
  /** How many of the shown cards have been handled (in order). */
  fallen: number;
  notRemoved: ReadonlySet<number>;
  lidOpen: boolean;
}) {
  const chips = entries.slice(0, STAGE_CHIPS);
  const landed = chips.map((_, i) => i < fallen && !notRemoved.has(i));

  return (
    <div
      className="ts-stage"
      data-lid={lidOpen ? "open" : "closed"}
      style={{ "--tone": TONE[mood] } as CSSProperties}
      aria-hidden
    >
      {(Object.keys(TONE) as StageMood[]).map((m) => (
        <div
          key={m}
          className="ts-light"
          data-on={m === mood}
          style={{ "--tone": TONE[m] } as CSSProperties}
        />
      ))}

      <div className="ts-scene">
        {/* Inside of the bin, visible behind the cards while the lid is open. */}
        <svg className="ts-bin" viewBox="0 0 120 120">
          <rect
            style={{ fill: "color-mix(in oklab, var(--foreground) 28%, transparent)" }}
            className="ts-mouth"
            x="29"
            y="36"
            width="62"
            height="7"
            rx="3.5"
          />
        </svg>

        {chips.map((entry, i) => {
          const [x, y, r] = SLOTS[i] ?? [0, 0, 0];
          const Icon = entryIcon(entry);
          const state = landed[i] ? "falling" : i < fallen ? "kept" : "resting";
          return (
            <div
              key={entry.name}
              className="ts-chip"
              data-state={state}
              style={
                {
                  "--x": `${x}px`,
                  "--y": `${y}px`,
                  "--r": `${r}deg`,
                  "--delay": `${80 + i * 45}ms`,
                } as CSSProperties
              }
            >
              <div className="relative flex h-full flex-col items-center justify-center gap-1 rounded-[7px] border border-border-strong bg-panel shadow-[0_1px_0_rgb(255_255_255/0.04)_inset,0_6px_14px_-8px_rgb(0_0_0/0.5)]">
                <Icon className="size-[18px] text-muted-foreground" strokeWidth={1.6} />
                <span className="tabular text-[9.5px] leading-none text-subtle-foreground">
                  {formatBytes(entry.bytes)}
                </span>
              </div>
            </div>
          );
        })}

        <svg className="ts-bin" viewBox="0 0 120 120">
          <defs>
            <linearGradient id="ts-body" x1="0" y1="0" x2="1" y2="1">
              <stop
                style={{ stopColor: "color-mix(in oklab, var(--panel) 70%, var(--foreground))" }}
                offset="0"
              />
              <stop style={{ stopColor: "var(--panel)" }} offset="0.35" />
              <stop
                style={{ stopColor: "color-mix(in oklab, var(--panel) 92%, black)" }}
                offset="1"
              />
            </linearGradient>
          </defs>
          <path
            style={{ fill: "url(#ts-body)", stroke: "var(--border-strong)" }}
            d="M29 40h62l-4.6 57.5a6.5 6.5 0 0 1-6.5 6H40.1a6.5 6.5 0 0 1-6.5-6Z"

            strokeWidth="1.5"
            strokeLinejoin="round"
          />
          <path
            style={{ stroke: "color-mix(in oklab, var(--foreground) 14%, transparent)" }}
            d="M34 46.5 37.4 96"

            strokeWidth="1.5"
            strokeLinecap="round"
          />
          <g
            style={{ stroke: "var(--subtle-foreground)" }}
            strokeWidth="2"
            strokeLinecap="round"
            opacity="0.7"
          >
            <path d="M49 54v36M60 54v36M71 54v36" />
          </g>
          <g className="ts-lid">
            <path
              style={{ stroke: "var(--border-strong)" }}
              d="M51 31v-3.5a4 4 0 0 1 4-4h10a4 4 0 0 1 4 4V31"
              fill="none"

              strokeWidth="2.5"
              strokeLinecap="round"
            />
            <rect
              style={{
                fill: "color-mix(in oklab, var(--panel) 80%, var(--foreground))",
                stroke: "var(--border-strong)",
              }}
              x="23"
              y="30.5"
              width="74"
              height="8"
              rx="4"

              strokeWidth="1.5"
            />
          </g>
          {mood === "done" ? (
            <g className="ts-badge">
              <circle
                style={{ fill: "var(--good)", stroke: "var(--panel-raised)" }}
                cx="92"
                cy="94"
                r="12"
                strokeWidth="3"
              />
              <path
                className="ts-check"
                d="M86.5 94.2l3.7 3.6 7.3-7.4"
                fill="none"
                stroke="white"
                strokeWidth="2.4"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </g>
          ) : mood === "locked" || mood === "attention" ? (
            <g className="ts-badge">
              <circle
                style={{ fill: "var(--warning)", stroke: "var(--panel-raised)" }}
                cx="92"
                cy="94"
                r="12"
                strokeWidth="3"
              />
              <path
                d="M92 88.5v6.5M92 99.2v.1"
                stroke="white"
                strokeWidth="2.6"
                strokeLinecap="round"
              />
            </g>
          ) : null}
        </svg>

        {landed.map((on, i) =>
          on ? (
            <div key={`m${i}`} className="ts-motes">
              {motes(i + 1).map((m, k) => (
                <span
                  key={k}
                  className="ts-mote"
                  style={
                    {
                      "--dx": m.dx,
                      "--dy": m.dy,
                      animationDelay: `${400 + k * 35}ms`,
                    } as CSSProperties
                  }
                />
              ))}
            </div>
          ) : null,
        )}
      </div>
    </div>
  );
}
