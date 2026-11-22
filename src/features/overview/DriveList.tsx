import { HardDrive, Usb } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Section } from "@/components/ui/section";
import { driveName } from "@/lib/drives";
import { formatBytes, formatPercent } from "@/lib/format";
import { samePath } from "@/lib/paths";
import { isWindows } from "@/lib/platform";
import { useApp } from "@/lib/store";

/**
 * Every drive's capacity, with a way to scan a whole drive. Windows only:
 * macOS lists just the home volume, which the storage strip already shows.
 */
export function DriveList() {
  const { drives, scanning, summary, scanFolder } = useApp();
  if (!isWindows() || drives.length < 2) return null;

  return (
    <Section title="Drives" description="Capacity of each drive. Scan one to see what fills it.">
      <ul className="-mx-2">
        {drives.map((d) => {
          const Icon = d.kind === "removable" ? Usb : HardDrive;
          const scanned = !!summary && samePath(summary.root, d.root);
          return (
            <li
              key={d.root}
              className="flex items-center gap-3 rounded-md px-2 py-1.5 transition-colors duration-[120ms] hover:bg-hover"
            >
              <Icon
                className="size-4 shrink-0 text-muted-foreground"
                strokeWidth={1.75}
                aria-hidden
              />
              <div className="min-w-0 flex-1">
                <div className="flex items-baseline justify-between gap-3">
                  <p className="truncate text-[13px]">{driveName(d)}</p>
                  <p className="tabular shrink-0 text-xs text-muted-foreground">
                    {formatBytes(d.availableBytes)} free of {formatBytes(d.totalBytes)}
                  </p>
                </div>
                <div
                  className="mt-1 h-[3px] overflow-hidden rounded-full bg-track"
                  role="img"
                  aria-label={`${formatPercent(d.usedBytes, d.totalBytes)} used`}
                >
                  <div
                    className="h-full rounded-full bg-accent"
                    style={{ width: `${d.totalBytes ? (d.usedBytes / d.totalBytes) * 100 : 0}%` }}
                  />
                </div>
              </div>
              <Button
                variant="ghost"
                size="sm"
                className="w-16"
                disabled={scanning}
                onClick={() => void scanFolder(d.root)}
              >
                {scanned ? "Rescan" : "Scan"}
              </Button>
            </li>
          );
        })}
      </ul>
    </Section>
  );
}
