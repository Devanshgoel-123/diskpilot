import { FolderOpen, KeyRound, Loader2, ShieldCheck } from "lucide-react";
import { type CSSProperties, useState } from "react";

import { Button } from "@/components/ui/button";
import { Panel } from "@/components/ui/panel";
import { api, errorMessage } from "@/lib/api";
import { formatBytes, formatCount, formatRelative, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { SecretsReport } from "@/lib/types";

/**
 * Offline check for API keys, tokens and private keys that ended up in AI
 * agents' conversation logs. Only masked values are ever shown.
 */
export function TranscriptSecrets() {
  const home = useApp((s) => s.env?.homeDir);
  const [report, setReport] = useState<SecretsReport | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const check = async () => {
    setRunning(true);
    setError(null);
    try {
      setReport(await api.checkAiTranscripts());
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setRunning(false);
    }
  };

  const found = report?.findings ?? [];
  const total = found.reduce((a, f) => a + f.matches.length, 0);

  return (
    <Panel>
      <div className="flex items-start gap-3 px-4 py-3.5">
        <KeyRound className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden />
        <div className="min-w-0 flex-1">
          <p className="text-[13px] font-medium">Credentials in AI transcripts</p>
          <p className="mt-0.5 text-xs text-muted-foreground">
            Claude Code, Codex CLI and Gemini CLI keep your conversations on disk, including keys
            you pasted or that commands printed. This check runs on this Mac only and shows masked
            values.
          </p>
          {error ? <p className="mt-2 text-xs text-critical">{error}</p> : null}
        </div>
        <Button variant="secondary" onClick={() => void check()} disabled={running}>
          {running ? <Loader2 className="animate-spin" aria-hidden /> : null}
          {report ? "Check again" : "Check transcripts"}
        </Button>
      </div>
      {report ? (
        <div className="border-t border-border px-4 py-3">
          {found.length === 0 ? (
            <p className="flex items-center gap-2 text-xs text-muted-foreground">
              <ShieldCheck className="size-3.5 text-[var(--tier-safe)]" aria-hidden />
              No known credential formats in {formatCount(report.filesChecked)} transcript files (
              {formatBytes(report.bytesChecked)}).
            </p>
          ) : (
            <>
              <p className="text-xs">
                {formatCount(total)} possible credential{total === 1 ? "" : "s"} in{" "}
                {formatCount(found.length)} of {formatCount(report.filesChecked)} transcript files.
                Revoke and replace each key with its provider: removing the transcript does not undo
                a leak.
              </p>
              <ul className="mt-2 divide-y divide-border rounded-lg border border-border">
                {found.map((f) => (
                  <li key={f.path} className="px-3 py-2">
                    <div className="flex items-center gap-2">
                      <span className="text-2xs text-subtle-foreground">{f.agent}</span>
                      <span className="selectable min-w-0 flex-1 truncate font-mono text-2xs text-muted-foreground">
                        {tildify(f.path, home)}
                      </span>
                      <span className="shrink-0 text-2xs text-subtle-foreground">
                        {formatRelative(f.modifiedMs)}
                      </span>
                      <button
                        type="button"
                        onClick={() => void api.revealInFinder(f.path)}
                        aria-label="Show in Finder"
                        title="Show in Finder"
                        className="shrink-0 text-subtle-foreground hover:text-foreground"
                      >
                        <FolderOpen className="size-3.5" aria-hidden />
                      </button>
                    </div>
                    <ul className="mt-1 space-y-0.5">
                      {f.matches.map((m) => (
                        <li
                          key={m.kind + m.masked}
                          className="tinted flex items-center gap-2 rounded px-2 py-1 text-xs"
                          style={{ "--tint": "var(--tier-review)" } as CSSProperties}
                        >
                          <span className="font-medium">{m.kind}</span>
                          <span className="font-mono text-2xs text-muted-foreground">
                            {m.masked}
                          </span>
                          <span className="ml-auto text-2xs text-subtle-foreground">
                            line {formatCount(m.line)}
                            {m.occurrences > 1 ? ` · ${m.occurrences} times` : ""}
                          </span>
                        </li>
                      ))}
                    </ul>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      ) : null}
    </Panel>
  );
}
