import { Bot } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { ASSISTANT_SETTINGS_ID } from "@/features/settings/AssistantSettings";
import { useApp } from "@/lib/store";

const DISMISSED_KEY = "diskpilot.assistant.promptDismissed";

function wasDismissed(): boolean {
  try {
    return localStorage.getItem(DISMISSED_KEY) === "true";
  } catch {
    return false;
  }
}

/**
 * Points people at the optional assistant, which is off by default and
 * otherwise only reachable from Settings. Hidden once it is set up or
 * dismissed.
 */
export function AssistantPrompt() {
  const enabled = useApp((s) => s.assistantEnabled);
  const status = useApp((s) => s.assistantStatus);
  const navigate = useApp((s) => s.navigate);
  const [dismissed, setDismissed] = useState(wasDismissed);

  // While the key status is loading, assume it is set up rather than flash.
  if (dismissed || (enabled && (status === null || status.providers.some((p) => p.configured))))
    return null;

  const setUp = () => {
    navigate("settings");
    requestAnimationFrame(() =>
      document.getElementById(ASSISTANT_SETTINGS_ID)?.scrollIntoView({ block: "start" }),
    );
  };

  const dismiss = () => {
    try {
      localStorage.setItem(DISMISSED_KEY, "true");
    } catch {
      // Not remembered; it hides for this session.
    }
    setDismissed(true);
  };

  return (
    <div className="flex items-center justify-between gap-4 rounded-[10px] border border-border px-3 py-2.5">
      <div className="flex min-w-0 gap-2 text-xs">
        <Bot className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" aria-hidden />
        <div>
          <p className="font-medium text-foreground">Ask about your storage in plain language</p>
          <p className="mt-0.5 text-muted-foreground">
            The optional assistant answers questions like “What can I safely clean up?” using your
            own API key from Anthropic, OpenAI or OpenRouter. It can only read DiskPilot’s results
            and never removes anything.
          </p>
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <Button variant="ghost" size="sm" onClick={dismiss}>
          Not now
        </Button>
        <Button variant="secondary" size="sm" onClick={setUp}>
          Set up assistant
        </Button>
      </div>
    </div>
  );
}
