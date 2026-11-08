import { ArrowUp, Bot, Loader2, RotateCcw, Settings2, ShieldCheck, Sparkles } from "lucide-react";
import {
  type CSSProperties,
  type KeyboardEvent,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { PageHeader } from "@/components/layout/PageHeader";
import { EmptyState } from "@/components/shared/EmptyState";
import { Markdown } from "@/components/shared/Markdown";
import { TierBadge } from "@/components/shared/TierBadge";
import { Button } from "@/components/ui/button";
import { Panel } from "@/components/ui/panel";
import { formatBytes, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import { MODEL_LABELS } from "@/lib/models";
import type { AssistantReply, Recommendation } from "@/lib/types";
import { words } from "@/lib/platform";

const suggestions = () => [
  `What is using the most space on my ${words().computer}?`,
  "What can I safely clean up right now?",
  "Which projects have node_modules I haven't touched in months?",
  "How much space does Docker use, and what could I remove?",
];

function Highlighted({ ids, recs }: { ids: string[]; recs: Recommendation[] }) {
  const home = useApp((s) => s.env?.homeDir);
  const review = useApp((s) => s.reviewInRecommendations);
  const items = ids
    .map((id) => recs.find((r) => r.id === id))
    .filter((r): r is Recommendation => !!r);
  if (!items.length) return null;
  const total = items.reduce((a, r) => a + r.estimatedReclaimBytes, 0);
  return (
    <div className="mt-3 rounded-lg border border-border bg-panel-raised">
      <ul className="divide-y divide-border">
        {items.map((r) => (
          <li key={r.id} className="flex items-center gap-3 px-3 py-2">
            <div className="min-w-0 flex-1">
              <p className="flex items-center gap-2 truncate text-xs">
                <span className="truncate">{r.title}</span>
                <TierBadge tier={r.tier} />
              </p>
              <p className="truncate font-mono text-2xs text-subtle-foreground">
                {tildify(r.path, home)}
              </p>
            </div>
            <span className="tabular shrink-0 text-xs">{formatBytes(r.bytes)}</span>
          </li>
        ))}
      </ul>
      <div className="flex items-center justify-between gap-3 border-t border-border px-3 py-2">
        <p className="text-2xs text-subtle-foreground">
          Nothing is removed until you preview and confirm.
        </p>
        <Button size="sm" onClick={() => review(items.map((r) => r.id))}>
          Review {items.length} · {formatBytes(total)}
        </Button>
      </div>
    </div>
  );
}

function AssistantMessage({ reply, recs }: { reply: AssistantReply; recs: Recommendation[] }) {
  return (
    <div className="flex gap-3">
      <span
        className="tinted mt-0.5 flex size-6 shrink-0 items-center justify-center rounded-md border"
        style={{ "--tint": "var(--accent)" } as CSSProperties}
      >
        <Sparkles className="size-3.5 text-accent" aria-hidden />
      </span>
      <div className="min-w-0 flex-1">
        <Markdown text={reply.text} />
        <Highlighted ids={reply.highlighted} recs={recs} />
        {reply.toolsUsed.length ? (
          <p className="mt-2 text-2xs text-subtle-foreground">
            Read: {reply.toolsUsed.join(" · ")}
            {MODEL_LABELS[reply.model] ? ` · ${MODEL_LABELS[reply.model]}` : ` · ${reply.model}`}
          </p>
        ) : null}
      </div>
    </div>
  );
}

export function AssistantPage() {
  const {
    assistantEnabled,
    assistantStatus,
    assistantProvider,
    assistantModel,
    providerModels,
    chat,
    asking,
    ask,
    resetChat,
    recommendations,
    navigate,
    refreshAssistantStatus,
  } = useApp();
  const [draft, setDraft] = useState("");
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void refreshAssistantStatus();
  }, [refreshAssistantStatus]);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [chat.length, asking]);

  const provider = assistantStatus?.providers.find((p) => p.id === assistantProvider);
  const configured = !!provider?.configured;
  const ready = assistantEnabled && configured && !!assistantModel;
  const providerLabel = provider?.label ?? "the provider";
  const modelLabel = useMemo(() => {
    const known = [...(provider?.models ?? []), ...(providerModels[assistantProvider] ?? [])];
    return (
      known.find((m) => m.id === assistantModel)?.label ??
      MODEL_LABELS[assistantModel] ??
      assistantModel
    );
  }, [provider, providerModels, assistantProvider, assistantModel]);

  const send = (text: string) => {
    if (!text.trim() || asking) return;
    void ask(text);
    setDraft("");
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      send(draft);
    }
  };

  if (!ready) {
    return (
      <>
        <PageHeader
          title="Assistant"
          description="Ask questions about your storage in plain language."
        />
        <EmptyState
          className="flex-1 justify-center"
          icon={Bot}
          title={
            !assistantEnabled
              ? "The assistant is off"
              : configured
                ? "Choose a model to start"
                : "Add an API key to start"
          }
          description={
            <>
              The assistant is optional. It uses your own API key from Anthropic, OpenAI or
              OpenRouter, stored in {words().keyStore}, and can only read DiskPilot's data: it
              cannot delete, move or run anything. When you ask a question, the question and the
              storage data needed to answer it (names, paths and sizes, never file contents) are
              sent to the provider you choose.
            </>
          }
          action={
            <Button variant="secondary" onClick={() => navigate("settings")}>
              <Settings2 />
              Set up in Settings
            </Button>
          }
        />
      </>
    );
  }

  return (
    <>
      <PageHeader
        title="Assistant"
        description={`${providerLabel} · ${modelLabel} · read-only access to DiskPilot's data`}
        actions={
          chat.length ? (
            <Button variant="ghost" onClick={() => void resetChat()} disabled={asking}>
              <RotateCcw />
              New conversation
            </Button>
          ) : null
        }
      />
      <div className="flex min-h-0 flex-1 flex-col">
        <div className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto max-w-[760px] space-y-6 px-6 py-6">
            {chat.length === 0 ? (
              <div className="pt-8">
                <p className="text-center text-sm font-medium">Ask about your storage</p>
                <p className="mt-1 text-center text-xs text-muted-foreground">
                  Answers come from the last scan, its recommendations, the developer tools analysis
                  and Docker.
                </p>
                <div className="mt-6 grid grid-cols-1 gap-2 sm:grid-cols-2">
                  {suggestions().map((s) => (
                    <button
                      key={s}
                      type="button"
                      onClick={() => send(s)}
                      className="rounded-lg border border-border bg-panel px-3 py-2.5 text-left text-xs text-muted-foreground transition-colors hover:bg-panel-raised hover:text-foreground"
                    >
                      {s}
                    </button>
                  ))}
                </div>
              </div>
            ) : null}
            {chat.map((entry, i) =>
              entry.role === "user" ? (
                <div key={i} className="flex justify-end">
                  <p className="selectable max-w-[80%] rounded-xl rounded-br-sm bg-accent-soft px-3.5 py-2 text-[13px] whitespace-pre-wrap">
                    {entry.text}
                  </p>
                </div>
              ) : entry.role === "assistant" ? (
                <AssistantMessage key={i} reply={entry.reply} recs={recommendations} />
              ) : (
                <p
                  key={i}
                  className="rounded-lg border border-border bg-panel px-3 py-2 text-xs text-critical"
                >
                  {entry.text}
                </p>
              ),
            )}
            {asking ? (
              <p className="flex items-center gap-2 text-xs text-muted-foreground">
                <Loader2 className="size-3.5 animate-spin" aria-hidden />
                Reading DiskPilot's data…
              </p>
            ) : null}
            <div ref={endRef} />
          </div>
        </div>
        <div className="shrink-0 border-t border-border bg-background px-6 py-3">
          <div className="mx-auto max-w-[760px]">
            <Panel className="flex items-end gap-2 p-2 focus-within:border-accent">
              <textarea
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={onKeyDown}
                rows={2}
                placeholder="Ask a question about your storage…"
                aria-label="Ask a question"
                className="selectable min-h-[40px] flex-1 resize-none bg-transparent px-1.5 py-1 text-[13px] placeholder:text-subtle-foreground focus:outline-none"
              />
              <Button
                size="icon"
                aria-label="Send"
                onClick={() => send(draft)}
                disabled={!draft.trim() || asking}
              >
                <ArrowUp />
              </Button>
            </Panel>
            <p className="mt-1.5 flex items-center gap-1.5 text-2xs text-subtle-foreground">
              <ShieldCheck className="size-3 text-good" aria-hidden />
              Read-only. Questions and storage metadata are sent to{" "}
              {provider?.host ?? providerLabel}; file contents never are.
            </p>
          </div>
        </div>
      </div>
    </>
  );
}
