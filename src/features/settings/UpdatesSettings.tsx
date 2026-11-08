import { Loader2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { formatRelative } from "@/lib/format";
import { useApp } from "@/lib/store";
import { useUpdates } from "@/lib/updates";
import { words } from "@/lib/platform";

import { Section, Setting } from "./parts";

export function UpdatesSettings() {
  const version = useApp((s) => s.env?.appVersion);
  const { autoCheck, setAutoCheck, checking, update, upToDate, lastCheckedMs, error, checkNow } =
    useUpdates();

  const status = checking
    ? "Checking…"
    : error
      ? `Could not check: ${error}`
      : update
        ? `DiskPilot ${update.version} is available. Install it from the sidebar.`
        : upToDate
          ? `You have the latest version (${version ?? "—"}).`
          : lastCheckedMs
            ? `Last checked ${formatRelative(lastCheckedMs)}.`
            : `You have version ${version ?? "—"}.`;

  return (
    <Section title="Updates">
      <Setting
        label="Check for updates automatically"
        description={`Once a day, DiskPilot asks GitHub whether a newer version exists. Nothing about you or this ${words().computer} is sent, and an update is installed only when you choose to, after its signature is verified.`}
        control={
          <input
            type="checkbox"
            role="switch"
            checked={autoCheck}
            onChange={(e) => setAutoCheck(e.target.checked)}
            aria-label="Check for updates automatically"
            className="size-4 accent-[var(--accent)]"
          />
        }
      />
      <Setting
        label="Check now"
        description={<span aria-live="polite">{status}</span>}
        control={
          <Button variant="secondary" disabled={checking} onClick={() => void checkNow(true)}>
            {checking ? <Loader2 className="animate-spin" /> : null}
            Check for updates
          </Button>
        }
      />
    </Section>
  );
}
