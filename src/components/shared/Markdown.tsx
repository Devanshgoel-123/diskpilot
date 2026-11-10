import { useMemo } from "react";

import { type Inline, parseMarkdown } from "@/lib/markdown";
import { cn } from "@/lib/utils";

function Inlines({ items }: { items: Inline[] }) {
  return (
    <>
      {items.map((i, idx) =>
        i.kind === "bold" ? (
          <strong key={idx} className="font-semibold text-foreground">
            {i.text}
          </strong>
        ) : i.kind === "code" ? (
          <code key={idx} className="rounded bg-panel-raised px-1 py-px font-mono text-[0.92em]">
            {i.text}
          </code>
        ) : (
          <span key={idx}>{i.text}</span>
        ),
      )}
    </>
  );
}

/** Renders the assistant's Markdown subset as React elements (no raw HTML). */
export function Markdown({ text, variant }: { text: string; variant?: "notes" }) {
  const blocks = useMemo(() => parseMarkdown(text), [text]);
  const notes = variant === "notes";
  return (
    <div
      className={
        notes
          ? "selectable space-y-2 text-[13px] leading-relaxed text-foreground/85"
          : "selectable space-y-2.5 text-[13px] leading-relaxed"
      }
    >
      {blocks.map((b, idx) => {
        switch (b.kind) {
          case "heading":
            return (
              <p
                key={idx}
                className={
                  notes ? "pt-3 text-xs font-semibold text-foreground first:pt-0" : "font-semibold"
                }
              >
                <Inlines items={b.inlines} />
              </p>
            );
          case "list": {
            const Tag = b.ordered ? "ol" : "ul";
            return (
              <Tag
                key={idx}
                className={cn(
                  b.ordered ? "list-decimal space-y-1 pl-5" : "list-disc space-y-1 pl-5",
                  notes && "space-y-1.5 marker:text-subtle-foreground",
                )}
              >
                {b.items.map((item, j) => (
                  <li key={j}>
                    <Inlines items={item} />
                  </li>
                ))}
              </Tag>
            );
          }
          case "codeblock":
            return (
              <pre
                key={idx}
                className="overflow-x-auto rounded-md border border-border bg-panel-raised p-2.5 font-mono text-xs"
              >
                {b.text}
              </pre>
            );
          default:
            return (
              <p key={idx}>
                <Inlines items={b.inlines} />
              </p>
            );
        }
      })}
    </div>
  );
}
