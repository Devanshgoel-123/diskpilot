import { Check, ChevronDown, HardDrive, House, Usb } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Menu, MenuContent, MenuItem, MenuSeparator, MenuTrigger } from "@/components/ui/menu";
import { driveFor, driveName, isDriveRoot } from "@/lib/drives";
import { formatBytes, tildify } from "@/lib/format";
import { samePath } from "@/lib/paths";
import { isWindows, words } from "@/lib/platform";
import { useApp } from "@/lib/store";

/**
 * Chooses what the next scan covers on Windows: the user folder or a whole
 * drive. Hidden on macOS, which lists only the home volume. Choosing does
 * not start a scan.
 */
export function DrivePicker() {
  const { drives, scanRoot, setScanRoot, scanning, env } = useApp();
  if (!isWindows() || drives.length === 0) return null;

  const home = env?.homeDir;
  const onDrive = isDriveRoot(scanRoot, drives) ? driveFor(scanRoot, drives) : null;
  const label = onDrive
    ? driveName(onDrive)
    : scanRoot
      ? tildify(scanRoot, home)
      : `Your ${words().homeFolder}`;

  return (
    <Menu>
      <MenuTrigger asChild>
        <Button
          variant="ghost"
          disabled={scanning}
          aria-label={`What to scan: ${label}`}
          className="max-w-[220px]"
        >
          {onDrive ? <HardDrive /> : <House />}
          <span className="truncate">{label}</span>
          <ChevronDown className="opacity-60" />
        </Button>
      </MenuTrigger>
      <MenuContent className="min-w-[260px]">
        <MenuItem onSelect={() => setScanRoot(null)} hint={home ?? null}>
          <House />
          <span className="flex-1">Your {words().homeFolder}</span>
          {!scanRoot ? <Check className="text-accent" /> : null}
        </MenuItem>
        <MenuSeparator />
        {drives.map((d) => {
          const chosen = !!scanRoot && samePath(scanRoot, d.root);
          return (
            <MenuItem
              key={d.root}
              onSelect={() => setScanRoot(d.root)}
              hint={`${formatBytes(d.availableBytes)} free of ${formatBytes(d.totalBytes)}${
                d.fileSystem ? ` · ${d.fileSystem}` : ""
              }`}
            >
              {d.kind === "removable" ? <Usb /> : <HardDrive />}
              <span className="flex-1">{driveName(d)}</span>
              {chosen ? <Check className="text-accent" /> : null}
            </MenuItem>
          );
        })}
      </MenuContent>
    </Menu>
  );
}
