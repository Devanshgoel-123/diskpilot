import { KeyRound, Loader2, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api, errorMessage } from "@/lib/api";
import { MODEL_LABELS } from "@/lib/models";
import { useApp } from "@/lib/store";
import { cn } from "@/lib/utils";

import { Section, Setting } from "./parts";

function Switch({
  on,
  onChange,
  label,
}: {
  on: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      onClick={() => onChange(!on)}
      className={cn(
        "relative h-5 w-9 rounded-full transition-colors",
        on ? "bg-accent" : "bg-border-strong",
      )}
    >
      <span
        className={cn(
          "absolute top-0.5 left-0.5 size-4 rounded-full bg-white shadow transition-transform",
          on && "translate-x-4",
        )}
      />
    </button>
  );
}

export function AssistantSettings() {
  const {
    assistantEnabled,
    setAssistantEnabled,
    assistantModel,
    setAssistantModel,
    assistantStatus,
    refreshAssistantStatus,
  } = useApp();
  const [key, setKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    void refreshAssistantStatus();
  }, [refreshAssistantStatus]);

  const save = async () => {
    setSaving(true);
    setMessage(null);
    try {
      await api.assistantSetKey(key);
      setKey("");
      setMessage({ ok: true, text: "Saved to the macOS Keychain." });
      await refreshAssistantStatus();
    } catch (e) {
      setMessage({ ok: false, text: errorMessage(e) });
    } finally {
      setSaving(false);
    }
  };

  const remove = async () => {
    try {
      await api.assistantClearKey();
      setMessage({ ok: true, text: "Key removed from the Keychain." });
      await refreshAssistantStatus();
    } catch (e) {
      setMessage({ ok: false, text: errorMessage(e) });
    }
  };

  const models = assistantStatus?.models ?? Object.keys(MODEL_LABELS);

  return (
    <Section title="Assistant (optional)">
      <Setting
        label="Enable the assistant"
        description="Ask questions about your storage in plain language. Off by default; every other DiskPilot feature works without it."
        control={
          <Switch
            on={assistantEnabled}
            onChange={setAssistantEnabled}
            label="Enable the assistant"
          />
        }
      />
      {assistantEnabled ? (
        <>
          <Setting
            label="Anthropic API key"
            description={
              <>
                {assistantStatus?.configured
                  ? assistantStatus.keySource === "environment"
                    ? "Using ANTHROPIC_API_KEY from the environment."
                    : "A key is stored in the macOS Keychain. DiskPilot never displays it."
                  : "Stored in the macOS Keychain and used only for assistant requests. Create one at console.anthropic.com."}
                {message ? (
                  <span className={cn("mt-1 block", message.ok ? "text-good" : "text-critical")}>
                    {message.text}
                  </span>
                ) : null}
              </>
            }
            control={
              <div className="flex items-center gap-1.5">
                {assistantStatus?.keySource === "keychain" ? (
                  <Button variant="ghost" onClick={() => void remove()}>
                    <Trash2 />
                    Remove
                  </Button>
                ) : null}
                <Input
                  type="password"
                  value={key}
                  onChange={(e) => setKey(e.target.value)}
                  placeholder={assistantStatus?.configured ? "Replace key" : "sk-ant-…"}
                  aria-label="Anthropic API key"
                  autoComplete="off"
                  spellCheck={false}
                  className="w-48"
                />
                <Button
                  variant="secondary"
                  onClick={() => void save()}
                  disabled={!key.trim() || saving}
                >
                  {saving ? <Loader2 className="animate-spin" /> : <KeyRound />}
                  Save
                </Button>
              </div>
            }
          />
          <Setting
            label="Model"
            description="Changing the model starts a new conversation."
            control={
              <select
                value={assistantModel}
                onChange={(e) => setAssistantModel(e.target.value)}
                aria-label="Assistant model"
                className="h-7 rounded-md border border-border-strong bg-panel px-2.5 text-[13px] focus-visible:border-accent focus-visible:outline-none"
              >
                {models.map((m) => (
                  <option key={m} value={m}>
                    {MODEL_LABELS[m] ?? m}
                  </option>
                ))}
              </select>
            }
          />
          <Setting
            label="What is sent"
            description="Your question, and the storage data the assistant reads to answer it: folder and file names, paths (with your home folder shown as ~), sizes and categories. File contents are never read or sent. Requests go directly from this Mac to Anthropic's API. The assistant has read-only tools and cannot change anything."
            control={null}
          />
        </>
      ) : null}
    </Section>
  );
}
