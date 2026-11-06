import { useEffect, useState } from "react";

import { WhatsNewDialog } from "@/components/shared/WhatsNewDialog";
import { Button } from "@/components/ui/button";
import { releaseNotes } from "@/lib/releaseNotes";
import { useApp } from "@/lib/store";

const SEEN_KEY = "diskpilot.lastSeenVersion";

/**
 * After an update, shows that version's release notes once. A first install
 * shows nothing: everything is new then.
 */
export function WhatsNewAfterUpdate() {
  const version = useApp((s) => s.env?.appVersion);
  const [shown, setShown] = useState<string | null>(null);

  useEffect(() => {
    if (!version) return;
    let seen: string | null;
    try {
      seen = localStorage.getItem(SEEN_KEY);
      localStorage.setItem(SEEN_KEY, version);
    } catch {
      return;
    }
    if (seen && seen !== version && releaseNotes(version)) {
      const t = setTimeout(() => setShown(version), 600);
      return () => clearTimeout(t);
    }
  }, [version]);

  if (!shown) return null;
  return (
    <WhatsNewDialog
      open
      title={`DiskPilot is now ${shown}`}
      subtitle="Here is what changed."
      notes={releaseNotes(shown)}
      onClose={() => setShown(null)}
      footer={<Button onClick={() => setShown(null)}>Got it</Button>}
    />
  );
}
