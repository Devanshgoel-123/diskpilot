import { LockKeyhole } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { formatCount } from "@/lib/format";
import { useApp } from "@/lib/store";

/** Protected folders worth naming, matched against unreadable paths. */
const NAMED: [RegExp, string][] = [
  [/\.photoslibrary$/i, "Photos"],
  [/\/Library\/Mail$/, "Mail"],
  [/\/Library\/Mobile Documents$/, "iCloud Drive"],
  [/\/Library\/Messages$/, "Messages"],
  [/\/Library\/Safari$/, "Safari"],
];

function list(names: string[]): string {
  if (names.length <= 1) return names.join("");
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/**
 * Shown when the scan could not read folders macOS protects. Only the user can
 * grant Full Disk Access; DiskPilot opens the right settings page.
 */
export function PrivateFoldersNotice() {
  const summary = useApp((s) => s.summary);
  const scanning = useApp((s) => s.scanning);
  const [access, setAccess] = useState<string | null>(null);
  const [asked, setAsked] = useState(false);

  useEffect(() => {
    const check = () =>
      void api
        .getFullDiskAccess()
        .then(setAccess)
        .catch(() => setAccess(null));
    check();
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
  }, []);

  const named = useMemo(() => {
    const paths = summary?.errorSamples.map((e) => e.path) ?? [];
    return NAMED.filter(([re]) => paths.some((p) => re.test(p)))
      .map(([, name]) => name)
      .slice(0, 3);
  }, [summary]);

  if (!summary || scanning || access !== "denied" || summary.errorCount === 0) return null;

  const others = summary.errorCount - named.length;
  return (
    <div
      role="status"
      className="flex items-start justify-between gap-4 rounded-[10px] border border-border bg-panel-raised px-3 py-2.5"
    >
      <div className="flex min-w-0 gap-2 text-xs">
        <LockKeyhole className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" aria-hidden />
        <div>
          <p className="text-foreground">
            {list([
              ...named,
              ...(others > 0
                ? [`${formatCount(others)} ${named.length ? "other " : ""}private folders`]
                : []),
            ])}{" "}
            weren&apos;t measured.
          </p>
          <p className="mt-0.5 text-muted-foreground">
            {asked
              ? "Turn on DiskPilot in the list, then quit and reopen DiskPilot and scan again."
              : "macOS lets DiskPilot read them only with Full Disk Access. Sizes are measured on this Mac, and the assistant, if you use it, sees only each folder's total."}
          </p>
        </div>
      </div>
      <Button
        size="sm"
        variant="secondary"
        onClick={() => {
          setAsked(true);
          void api.openFullDiskAccessSettings();
        }}
      >
        Open Privacy Settings
      </Button>
    </div>
  );
}
