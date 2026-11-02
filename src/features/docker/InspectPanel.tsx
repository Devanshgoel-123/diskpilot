import { Loader2, X } from "lucide-react";
import { useEffect, useState } from "react";

import { CopyButton } from "@/components/shared/CopyButton";
import { Button } from "@/components/ui/button";
import { api, errorMessage } from "@/lib/api";

export interface InspectTarget {
  kind: "container" | "image" | "volume";
  id: string;
  title: string;
}

/** Read-only `docker inspect` output for one object. */
export function InspectPanel({ target, onClose }: { target: InspectTarget; onClose: () => void }) {
  return <InspectBody key={`${target.kind}:${target.id}`} target={target} onClose={onClose} />;
}

function InspectBody({ target, onClose }: { target: InspectTarget; onClose: () => void }) {
  const [json, setJson] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    api
      .dockerInspect(target.kind, target.id)
      .then((j) => live && setJson(j))
      .catch((e: unknown) => live && setError(errorMessage(e)));
    return () => {
      live = false;
    };
  }, [target]);

  return (
    <aside className="flex w-[420px] shrink-0 flex-col border-l border-border bg-panel">
      <div className="flex items-center justify-between gap-2 border-b border-border px-4 py-2.5">
        <div className="min-w-0">
          <p className="text-2xs tracking-wide text-subtle-foreground uppercase">
            Inspect {target.kind}
          </p>
          <p className="truncate text-[13px] font-medium">{target.title}</p>
        </div>
        <div className="flex shrink-0 items-center">
          {json ? <CopyButton text={json} /> : null}
          <Button variant="ghost" size="icon" onClick={onClose} aria-label="Close inspector">
            <X />
          </Button>
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-3">
        {error ? <p className="text-xs text-critical">{error}</p> : null}
        {!json && !error ? (
          <p className="flex items-center gap-2 text-xs text-muted-foreground">
            <Loader2 className="size-3.5 animate-spin" aria-hidden /> Loading…
          </p>
        ) : null}
        {json ? (
          <pre className="selectable font-mono text-2xs leading-relaxed whitespace-pre text-muted-foreground">
            {json}
          </pre>
        ) : null}
      </div>
    </aside>
  );
}
