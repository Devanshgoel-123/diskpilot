import { openUrl } from "@tauri-apps/plugin-opener";
import { ChevronRight, ExternalLink, ShieldCheck } from "lucide-react";
import { useEffect, useState } from "react";

import { CopyButton } from "@/components/shared/CopyButton";
import { Button } from "@/components/ui/button";
import { Section } from "@/components/ui/section";
import type { SystemExtra } from "@/lib/types";

import { sizeLabel, useSystemExtras, visibleExtras } from "./extrasStore";

function CommandLine({ label, command }: { label: string; command: string }) {
  return (
    <div className="mt-2">
      <p className="text-2xs text-subtle-foreground">{label}</p>
      <div className="mt-1 flex items-center gap-2 rounded-md border border-border bg-panel px-2 py-1">
        <code className="selectable min-w-0 flex-1 truncate font-mono text-2xs" title={command}>
          {command}
        </code>
        <CopyButton text={command} />
      </div>
    </div>
  );
}

function ExtraRow({ extra }: { extra: SystemExtra }) {
  const [open, setOpen] = useState(false);
  const settingsUri = extra.settingsUri;
  return (
    <li className="border-b border-border last:border-b-0">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="flex w-full items-center gap-3 px-2 py-2.5 text-left hover:bg-panel-raised"
      >
        <ChevronRight
          className={`size-3.5 shrink-0 text-muted-foreground transition-transform ${open ? "rotate-90" : ""}`}
          aria-hidden
        />
        <span className="min-w-0 flex-1 truncate text-[13px] font-medium">{extra.label}</span>
        <span className="tabular shrink-0 text-[13px] text-muted-foreground">
          {sizeLabel(extra)}
        </span>
      </button>
      {open && (
        <div className="px-2 pb-3 pl-8 text-xs">
          <p className="text-muted-foreground">{extra.explanation}</p>
          {extra.sizeNote && <p className="mt-1 text-subtle-foreground">{extra.sizeNote}</p>}
          <p className="mt-2 text-foreground">{extra.howToReclaim}</p>
          {extra.checkCommand && (
            <CommandLine label="See the real size first" command={extra.checkCommand} />
          )}
          {extra.command && (
            <CommandLine
              label={extra.needsAdmin ? "Run in an administrator terminal" : "Run in a terminal"}
              command={extra.command}
            />
          )}
          {settingsUri && (
            <Button
              size="sm"
              variant="secondary"
              className="mt-2"
              onClick={() => void openUrl(settingsUri)}
            >
              <ExternalLink className="size-3.5" aria-hidden />
              Open Settings
            </Button>
          )}
          {extra.paths.length > 0 && (
            <p className="selectable mt-2 font-mono text-2xs text-subtle-foreground">
              {extra.paths.join(" · ")}
            </p>
          )}
        </div>
      )}
    </li>
  );
}

/**
 * Windows system areas on the Overview: what they are, how big, and the
 * Windows tool that reclaims each. DiskPilot never removes these itself.
 */
export function SystemExtrasSection() {
  const { extras, load } = useSystemExtras();
  useEffect(() => void load(), [load]);

  const shown = extras ? visibleExtras(extras) : [];
  if (shown.length === 0) return null;
  return (
    <Section
      title="Windows system files"
      description="Space Windows manages itself. DiskPilot shows the size and the Windows tool that frees it, and never removes these files."
      actions={
        <span className="flex items-center gap-1 text-2xs text-subtle-foreground">
          <ShieldCheck className="size-3.5" aria-hidden />
          Not removed by DiskPilot
        </span>
      }
    >
      <ul className="rounded-[10px] border border-border">
        {shown.map((e) => (
          <ExtraRow key={e.id} extra={e} />
        ))}
      </ul>
    </Section>
  );
}
