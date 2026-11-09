import { openUrl } from "@tauri-apps/plugin-opener";
import { KeyRound, Loader2, RefreshCw, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useShallow } from "zustand/react/shallow";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api, errorMessage } from "@/lib/api";
import { MODEL_LABELS } from "@/lib/models";
import { ASSISTANT_PROVIDERS, useApp } from "@/lib/store";
import type { AssistantProvider, ModelInfo, ProviderStatus } from "@/lib/types";
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

const PROVIDER_LABELS: Record<AssistantProvider, string> = {
  anthropic: "Anthropic",
  openai: "OpenAI",
  openrouter: "OpenRouter",
};

/** Segmented control for choosing the provider, with a dot for those that have a key. */
function ProviderPicker({
  value,
  onChange,
  statuses,
}: {
  value: AssistantProvider;
  onChange: (p: AssistantProvider) => void;
  statuses: ProviderStatus[];
}) {
  return (
    <div
      role="radiogroup"
      aria-label="Assistant provider"
      className="flex rounded-md border border-border-strong bg-panel-raised p-0.5"
    >
      {ASSISTANT_PROVIDERS.map((p) => {
        const configured = statuses.find((s) => s.id === p)?.configured;
        const selected = p === value;
        return (
          <button
            key={p}
            type="button"
            role="radio"
            aria-checked={selected}
            onClick={() => onChange(p)}
            className={cn(
              "flex h-6 items-center gap-1.5 rounded-[5px] px-2.5 text-[13px] transition-colors",
              selected
                ? "bg-panel text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            {PROVIDER_LABELS[p]}
            {configured ? (
              <span className="size-1.5 rounded-full bg-good" aria-label="key saved" />
            ) : null}
          </button>
        );
      })}
    </div>
  );
}

const selectClass =
  "h-7 max-w-64 rounded-md border border-border-strong bg-panel px-2.5 text-[13px] focus-visible:border-accent focus-visible:outline-none";

/** Anchor the Overview links to. */
export const ASSISTANT_SETTINGS_ID = "assistant-settings";

export function AssistantSettings() {
  const {
    assistantEnabled,
    setAssistantEnabled,
    assistantProvider: provider,
    setAssistantProvider,
    assistantModel,
    setAssistantModel,
    providerModels,
    loadProviderModels,
    assistantStatus,
    refreshAssistantStatus,
  } = useApp(
    useShallow((s) => ({
      assistantEnabled: s.assistantEnabled,
      setAssistantEnabled: s.setAssistantEnabled,
      assistantProvider: s.assistantProvider,
      setAssistantProvider: s.setAssistantProvider,
      assistantModel: s.assistantModel,
      setAssistantModel: s.setAssistantModel,
      providerModels: s.providerModels,
      loadProviderModels: s.loadProviderModels,
      assistantStatus: s.assistantStatus,
      refreshAssistantStatus: s.refreshAssistantStatus,
    })),
  );
  const [key, setKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [checking, setChecking] = useState(false);
  const [filter, setFilter] = useState("");
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    void refreshAssistantStatus();
  }, [refreshAssistantStatus]);

  const status = assistantStatus?.providers.find((p) => p.id === provider);
  const label = PROVIDER_LABELS[provider];
  const fixedModels = status?.models.length ? status.models : null;
  const listed = providerModels[provider];

  const check = useCallback(
    async (p: AssistantProvider, quiet = false) => {
      setChecking(true);
      try {
        const models = await loadProviderModels(p);
        if (!quiet) {
          setMessage({
            ok: true,
            text: fixedModels
              ? `Connected to ${PROVIDER_LABELS[p]}.`
              : `Connected to ${PROVIDER_LABELS[p]} · ${models.length} models can use DiskPilot's tools.`,
          });
        }
      } catch (e) {
        setMessage({ ok: false, text: errorMessage(e) });
      } finally {
        setChecking(false);
      }
    },
    [loadProviderModels, fixedModels],
  );

  const choose = (p: AssistantProvider) => {
    setAssistantProvider(p);
    setKey("");
    setFilter("");
    setMessage(null);
  };

  const save = async () => {
    setSaving(true);
    setMessage(null);
    try {
      await api.assistantSetKey(provider, key);
      setKey("");
      await refreshAssistantStatus();
      setMessage({ ok: true, text: "Saved to the macOS Keychain. Checking the key…" });
      await check(provider);
    } catch (e) {
      setMessage({ ok: false, text: errorMessage(e) });
    } finally {
      setSaving(false);
    }
  };

  const remove = async () => {
    try {
      await api.assistantClearKey(provider);
      setMessage({ ok: true, text: "Key removed from the Keychain." });
      await refreshAssistantStatus();
    } catch (e) {
      setMessage({ ok: false, text: errorMessage(e) });
    }
  };

  const options: ModelInfo[] = useMemo(() => {
    const all = fixedModels ?? listed ?? [];
    const q = filter.trim().toLowerCase();
    const shown = q
      ? all.filter((m) => m.id.toLowerCase().includes(q) || m.label.toLowerCase().includes(q))
      : all;
    // Keep the chosen model visible even when filtered out or not yet listed.
    if (assistantModel && !shown.some((m) => m.id === assistantModel)) {
      const chosen = all.find((m) => m.id === assistantModel) ?? {
        id: assistantModel,
        label: MODEL_LABELS[assistantModel] ?? assistantModel,
      };
      return [chosen, ...shown];
    }
    return shown;
  }, [fixedModels, listed, filter, assistantModel]);

  const keyDescription = status?.configured
    ? status.keySource === "environment"
      ? `Using ${status.envVar} from the environment.`
      : "A key is stored in the macOS Keychain. DiskPilot never displays it."
    : "Stored in the macOS Keychain and used only for assistant requests.";

  return (
    <Section title="Assistant (optional)" id={ASSISTANT_SETTINGS_ID}>
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
            label="Provider"
            description="Use your own key from Anthropic, OpenAI or OpenRouter. OpenRouter gives one key for models from many companies."
            control={
              <ProviderPicker
                value={provider}
                onChange={choose}
                statuses={assistantStatus?.providers ?? []}
              />
            }
          />
          <Setting
            label={`${label} API key`}
            description={
              <>
                {keyDescription}{" "}
                {status ? (
                  <button
                    type="button"
                    className="text-accent hover:underline"
                    onClick={() => void openUrl(status.consoleUrl)}
                  >
                    Create a key
                  </button>
                ) : null}
                {message ? (
                  <span className={cn("mt-1 block", message.ok ? "text-good" : "text-critical")}>
                    {message.text}
                  </span>
                ) : null}
              </>
            }
            control={
              <div className="flex items-center gap-1.5">
                {status?.keySource === "keychain" ? (
                  <Button variant="ghost" onClick={() => void remove()}>
                    <Trash2 />
                    Remove
                  </Button>
                ) : null}
                <Input
                  type="password"
                  value={key}
                  onChange={(e) => setKey(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && key.trim() && !saving) void save();
                  }}
                  placeholder={status?.configured ? "Replace key" : `${status?.keyPrefix ?? ""}…`}
                  aria-label={`${label} API key`}
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
            description={
              fixedModels
                ? "Changing the model starts a new conversation."
                : listed
                  ? "Models that can use DiskPilot's read-only tools, as listed by the provider. Changing the model starts a new conversation."
                  : status?.configured
                    ? "Check the connection to list the models this key can use."
                    : `Save your ${label} key to list its models.`
            }
            control={
              <div className="flex items-center gap-1.5">
                {!fixedModels && (listed?.length ?? 0) > 20 ? (
                  <Input
                    value={filter}
                    onChange={(e) => setFilter(e.target.value)}
                    placeholder="Filter models"
                    aria-label="Filter models"
                    spellCheck={false}
                    className="w-32"
                  />
                ) : null}
                {options.length ? (
                  <select
                    value={assistantModel}
                    onChange={(e) => setAssistantModel(e.target.value)}
                    aria-label="Assistant model"
                    className={selectClass}
                  >
                    {!assistantModel ? <option value="">Choose a model</option> : null}
                    {options.map((m) => (
                      <option key={m.id} value={m.id}>
                        {m.label}
                      </option>
                    ))}
                  </select>
                ) : null}
                {status?.configured ? (
                  <Button
                    variant="ghost"
                    onClick={() => void check(provider)}
                    disabled={checking}
                    aria-label="Check the connection and refresh the model list"
                  >
                    {checking ? <Loader2 className="animate-spin" /> : <RefreshCw />}
                    Check
                  </Button>
                ) : null}
              </div>
            }
          />
          <Setting
            label="What is sent"
            description={`Your question, and the storage data the assistant reads to answer it: folder and file names, paths (with your home folder shown as ~), sizes and categories. File contents are never read or sent. Requests go directly from this Mac to ${status?.host ?? `${label}'s API`}, and your key is sent only there. The assistant has read-only tools and cannot change anything.`}
            control={null}
          />
        </>
      ) : null}
    </Section>
  );
}
