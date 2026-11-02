import { CornerDownRight } from "lucide-react";

import { CONFIDENCE_LABEL } from "@/lib/categories";
import { tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { Classification } from "@/lib/types";

import { CategoryBadge } from "./CategoryBadge";

/** Shows what an item is and why DiskPilot thinks so. */
export function ClassificationCard({
  classification,
  itemPath,
}: {
  classification: Classification;
  itemPath?: string;
}) {
  const home = useApp((s) => s.env?.homeDir);
  const inherited =
    classification.matchedPath && classification.matchedPath !== itemPath
      ? classification.matchedPath
      : null;
  return (
    <div className="rounded-md border border-border bg-panel-raised p-3">
      <div className="flex flex-wrap items-center gap-1.5">
        <CategoryBadge category={classification.category} />
        <span className="text-2xs text-subtle-foreground">
          {CONFIDENCE_LABEL[classification.confidence]}
        </span>
      </div>
      <p className="mt-2 text-[13px] font-medium">{classification.label}</p>
      <p className="selectable mt-1 text-xs leading-relaxed text-muted-foreground">
        {classification.explanation}
      </p>
      {inherited ? (
        <p className="mt-2 flex items-start gap-1 text-2xs text-subtle-foreground">
          <CornerDownRight className="mt-px size-3 shrink-0" aria-hidden />
          <span className="min-w-0 break-all">
            Inherited from <span className="font-mono">{tildify(inherited, home)}</span>
          </span>
        </p>
      ) : null}
      <p className="mt-2 font-mono text-2xs text-subtle-foreground">
        rule: {classification.ruleId}
      </p>
    </div>
  );
}
