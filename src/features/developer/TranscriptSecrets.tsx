import { FolderOpen, KeyRound, Loader2, ShieldCheck } from "lucide-react";
import { type CSSProperties, useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Panel } from "@/components/ui/panel";
import { api, errorMessage } from "@/lib/api";
import { formatBytes, formatCount, formatRelative, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { SecretsReport, TranscriptFinding } from "@/lib/types";
import { words } from "@/lib/platform";

/** Matches the backend: a transcript used this recently may still be open. */
const OPEN_WINDOW_MS = 2 * 60 * 1000;

/**
 * Offline check for API keys, tokens and private keys that ended up in AI
 * agents' conversation logs. Only masked values are ever shown.
 */
export function TranscriptSecrets() {
  const home = useApp((s) => s.env?.homeDir);
  const [report, setReport] = useState<SecretsReport | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<TranscriptFinding | null>(null);
  const [removing, setRemoving] = useState(false);

  const remove = async (f: TranscriptFinding) => {
    setRemoving(true);
    setError(null);
    try {
      setReport(await api.removeTranscriptSecrets(f.path));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setRemoving(false);
      setConfirm(null);
    }
  };

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
            you pasted or that commands printed. This check runs on this {words().computer} only and
            shows masked values.
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
                Revoke and replace each key with its provider: removing it from the transcript does
                not undo a leak.
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
                      <RemoveButton
                        finding={f}
                        checkedAtMs={report.generatedAtMs}
                        onClick={() => setConfirm(f)}
                      />
                      <button
                        type="button"
                        onClick={() => void api.revealInFinder(f.path)}
                        aria-label={words().showInFileManager}
                        title={words().showInFileManager}
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
      <Dialog open={!!confirm} onOpenChange={(o) => !o && !removing && setConfirm(null)}>
        <DialogContent className="w-[min(460px,calc(100vw-32px))]" showClose={!removing}>
          {confirm ? (
            <>
              <DialogHeader>
                <DialogTitle>Remove keys from this transcript?</DialogTitle>
                <DialogDescription asChild>
                  <div className="space-y-2">
                    <p>
                      DiskPilot replaces {confirm.matches.length === 1 ? "the key" : "each key"} in{" "}
                      <span className="font-mono text-2xs">{tildify(confirm.path, home)}</span> with
                      “[removed by DiskPilot]”. The rest of the conversation stays, and you can
                      still resume it.
                    </p>
                    <p>
                      No copy of the old file is kept, so this can’t be undone. Anyone or anything
                      that already read the key still has it: revoke it with its provider too.
                    </p>
                  </div>
                </DialogDescription>
              </DialogHeader>
              <DialogFooter>
                <Button variant="secondary" onClick={() => setConfirm(null)} disabled={removing}>
                  Cancel
                </Button>
                <Button
                  variant="destructive"
                  disabled={removing}
                  onClick={() => void remove(confirm)}
                >
                  {removing ? <Loader2 className="animate-spin" aria-hidden /> : null}
                  Remove keys
                </Button>
              </DialogFooter>
            </>
          ) : null}
        </DialogContent>
      </Dialog>
    </Panel>
  );
}

function RemoveButton({
  finding,
  checkedAtMs,
  onClick,
}: {
  finding: TranscriptFinding;
  /** When the transcripts were checked; the backend checks again on removal. */
  checkedAtMs: number;
  onClick: () => void;
}) {
  const open = finding.modifiedMs != null && checkedAtMs - finding.modifiedMs < OPEN_WINDOW_MS;
  return (
    <Button
      size="sm"
      variant="secondary"
      className="shrink-0"
      disabled={open}
      title={open ? "Used in the last two minutes. Close the conversation first." : undefined}
      onClick={onClick}
    >
      Remove keys
    </Button>
  );
}
