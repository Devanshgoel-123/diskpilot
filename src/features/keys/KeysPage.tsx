import {
  AlertTriangle,
  Cloud,
  Database,
  FileKey2,
  FileLock2,
  FolderOpen,
  KeyRound,
  Loader2,
  Lock,
  MoreHorizontal,
  Search,
  ShieldCheck,
  Tag,
  Trash2,
  X,
  type LucideIcon,
} from "lucide-react";
import { type CSSProperties, useEffect, useMemo, useState } from "react";
import { useShallow } from "zustand/react/shallow";

import { PageHeader } from "@/components/layout/PageHeader";
import { EmptyState } from "@/components/shared/EmptyState";
import { Pager } from "@/components/shared/Pager";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Menu, MenuContent, MenuItem, MenuSeparator, MenuTrigger } from "@/components/ui/menu";
import { Panel } from "@/components/ui/panel";
import { api } from "@/lib/api";
import { formatCount, formatRelative, tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { KeyFile, KeyKind } from "@/lib/types";
import { usePage } from "@/lib/usePage";
import { cn } from "@/lib/utils";

import { useKeys } from "./keysStore";

// ---- Pieces ---------------------------------------------------------------------

const KIND_ICON: Record<KeyKind, LucideIcon> = {
  sshKey: KeyRound,
  privateKey: FileKey2,
  keyBundle: FileLock2,
  certificate: ShieldCheck,
  envFile: FileKey2,
  cloudCredentials: Cloud,
  passwordDatabase: Database,
};

type Filter = "all" | "attention" | "unlabelled";
const PAGE_SIZE = 12;

const needsAttention = (f: KeyFile) => f.risks.some((r) => r.serious);

function FilterButton({
  label,
  value,
  warn,
  active,
  onClick,
}: {
  label: string;
  value: number;
  warn?: boolean;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        "flex h-6 items-center gap-1.5 rounded-md px-2.5 text-[13px] transition-colors duration-[120ms]",
        active
          ? "bg-panel text-foreground shadow-[0_0_0_0.5px_var(--border-strong),0_1px_2px_rgb(0_0_0/0.08)] dark:bg-selected"
          : "text-muted-foreground hover:text-foreground",
      )}
    >
      {warn ? <span className="size-1.5 rounded-full bg-tier-review" aria-hidden /> : null}
      {label}
      <span className="tabular text-xs text-subtle-foreground">{formatCount(value)}</span>
    </button>
  );
}

/** What makes the files risky, counted by kind, for the line under the answer. */
function riskSummary(files: KeyFile[]): string {
  const count = (k: string) =>
    files.filter((f) => f.risks.some((r) => r.kind === k && r.serious)).length;
  const parts: string[] = [];
  const others = count("othersCanRead");
  const share = count("easyToShare");
  const git = count("inGitProject");
  if (others) parts.push(`${others} can be read by other accounts`);
  if (share)
    parts.push(`${share} ${share === 1 ? "sits" : "sit"} where files get shared or synced`);
  if (git)
    parts.push(`${git} ${git === 1 ? "is" : "are"} in a git project without a .gitignore entry`);
  return parts.join(" · ");
}

function LabelDialog({ file, onClose }: { file: KeyFile | null; onClose: () => void }) {
  const providers = useKeys((s) => s.providers);
  const change = useKeys((s) => s.change);
  const busy = useKeys((s) => s.busy);
  const [provider, setProvider] = useState("");
  const [note, setNote] = useState("");
  const [openFor, setOpenFor] = useState<string | null>(null);
  // Start from the file's current label each time the dialog opens.
  if (file && openFor !== file.path) {
    setOpenFor(file.path);
    setProvider(file.label?.provider ?? "");
    setNote(file.label?.note ?? "");
  }
  if (!file && openFor !== null) setOpenFor(null);

  const save = async (p: string, n: string) => {
    if (!file) return;
    if (await change(`label:${file.path}`, () => api.keysSetLabel(file.path, p, n))) onClose();
  };

  return (
    <Dialog open={!!file} onOpenChange={(o) => !o && !busy && onClose()}>
      <DialogContent className="w-[min(480px,calc(100vw-32px))]">
        {file ? (
          <>
            <DialogHeader>
              <DialogTitle>Label {file.name}</DialogTitle>
              <DialogDescription>
                Say what this key is for, so you can find it later by searching for it.
              </DialogDescription>
            </DialogHeader>
            <div className="space-y-4 px-5 py-4">
              <div>
                <p className="mb-1.5 text-xs font-medium text-muted-foreground">Provider</p>
                <div className="flex flex-wrap gap-1.5">
                  {providers.map((p) => (
                    <button
                      key={p}
                      type="button"
                      aria-pressed={provider === p}
                      onClick={() => setProvider(p)}
                      className={cn(
                        "h-7 rounded-md border px-2.5 text-[13px] transition-colors",
                        provider === p
                          ? "border-accent bg-[color-mix(in_oklab,var(--accent)_12%,transparent)] font-medium text-foreground"
                          : "border-border-strong text-muted-foreground hover:bg-hover hover:text-foreground",
                      )}
                    >
                      {p}
                    </button>
                  ))}
                </div>
                {provider === "Other" || (provider && !providers.includes(provider)) ? (
                  <Input
                    className="mt-2"
                    placeholder="Name of the service"
                    maxLength={40}
                    value={provider === "Other" ? "" : provider}
                    onChange={(e) => setProvider(e.target.value || "Other")}
                    aria-label="Name of the service"
                  />
                ) : null}
              </div>
              <div>
                <label
                  htmlFor="key-note"
                  className="mb-1.5 block text-xs font-medium text-muted-foreground"
                >
                  Note (optional)
                </label>
                <Input
                  id="key-note"
                  placeholder="e.g. production deploys, expires March 2027"
                  maxLength={120}
                  value={note}
                  onChange={(e) => setNote(e.target.value)}
                />
                <p className="mt-1.5 flex items-center gap-1.5 text-xs text-muted-foreground">
                  <AlertTriangle className="size-3 text-tier-review" aria-hidden />
                  Never paste the key or a password here.
                </p>
              </div>
            </div>
            <DialogFooter className="justify-between">
              {file.label ? (
                <Button
                  variant="ghost"
                  className="mr-auto text-critical hover:bg-critical/10 hover:text-critical"
                  disabled={!!busy}
                  onClick={() => void save("", "")}
                >
                  Remove label
                </Button>
              ) : (
                <span className="mr-auto" />
              )}
              <Button variant="secondary" onClick={onClose} disabled={!!busy}>
                Cancel
              </Button>
              <Button
                disabled={!!busy || !provider.trim() || provider === "Other"}
                onClick={() => void save(provider, note)}
              >
                {busy ? <Loader2 className="animate-spin" /> : null}
                Save label
              </Button>
            </DialogFooter>
          </>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

function TrashDialog({ file, onClose }: { file: KeyFile | null; onClose: () => void }) {
  const change = useKeys((s) => s.change);
  const busy = useKeys((s) => s.busy);
  const home = useApp((s) => s.env?.homeDir);
  return (
    <Dialog open={!!file} onOpenChange={(o) => !o && !busy && onClose()}>
      <DialogContent className="w-[min(460px,calc(100vw-32px))]" showClose={!busy}>
        {file ? (
          <>
            <DialogHeader>
              <DialogTitle>Move {file.name} to the Trash?</DialogTitle>
              <DialogDescription asChild>
                <div className="space-y-2">
                  <p className="font-mono text-xs break-all text-foreground">
                    {tildify(file.path, home)}
                  </p>
                  <p>
                    Anything that still uses this {file.kindLabel.toLowerCase()} stops working. You
                    can put it back from the Trash until you empty it.
                  </p>
                </div>
              </DialogDescription>
            </DialogHeader>
            <DialogFooter>
              <Button variant="secondary" onClick={onClose} disabled={!!busy}>
                Cancel
              </Button>
              <Button
                variant="destructive"
                disabled={!!busy}
                onClick={async () => {
                  await change(`trash:${file.path}`, () => api.keysMoveToTrash(file.path));
                  onClose();
                }}
              >
                {busy ? <Loader2 className="animate-spin" /> : <Trash2 />}
                Move to Trash
              </Button>
            </DialogFooter>
          </>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

function KeyRow({
  file,
  onLabel,
  onTrash,
}: {
  file: KeyFile;
  onLabel: (f: KeyFile) => void;
  onTrash: (f: KeyFile) => void;
}) {
  const home = useApp((s) => s.env?.homeDir);
  const busy = useKeys((s) => s.busy);
  const change = useKeys((s) => s.change);
  const Icon = KIND_ICON[file.kind];
  const serious = file.risks.filter((r) => r.serious);
  const minor = file.risks.filter((r) => !r.serious);
  const folder = tildify(file.path.slice(0, file.path.lastIndexOf("/")) || "/", home);
  return (
    <li className="grid grid-cols-1 items-start gap-x-6 gap-y-2 px-3 py-2.5 md:grid-cols-[minmax(0,1fr)_minmax(0,1.15fr)_210px]">
      <div className="flex min-w-0 gap-3">
        <span className="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-md bg-panel-raised">
          <Icon className="size-3.5 text-muted-foreground" aria-hidden />
        </span>
        <div className="min-w-0">
          <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
            <span className="truncate font-mono text-[12.5px] font-medium">{file.name}</span>
            <span className="shrink-0 text-xs text-subtle-foreground">{file.kindLabel}</span>
            {file.label ? (
              <button
                type="button"
                onClick={() => onLabel(file)}
                title={file.label.note || "Edit label"}
                style={{ "--tint": "var(--accent)" } as CSSProperties}
                className="tinted tint-text inline-flex shrink-0 items-center gap-1 rounded-full border px-2 py-px text-xs font-medium"
              >
                <Tag className="size-3" aria-hidden />
                {file.label.provider}
                {file.label.note ? (
                  <span className="max-w-40 truncate font-normal opacity-80">
                    · {file.label.note}
                  </span>
                ) : null}
              </button>
            ) : (
              <button
                type="button"
                onClick={() => onLabel(file)}
                className="inline-flex shrink-0 items-center gap-1 rounded-full border border-dashed border-border-strong px-2 py-px text-xs text-muted-foreground hover:bg-hover hover:text-foreground"
              >
                <Tag className="size-3" aria-hidden />
                Add label
              </button>
            )}
          </div>
          <p
            className="mt-0.5 truncate font-mono text-2xs text-subtle-foreground"
            title={file.path}
          >
            {folder}
            {file.modifiedMs ? (
              <span className="font-sans"> · changed {formatRelative(file.modifiedMs)}</span>
            ) : null}
          </p>
        </div>
      </div>
      <ul className="flex min-w-0 flex-wrap gap-1.5 md:pt-0.5" aria-label="Why it's risky">
        {serious.map((r) => (
          <li
            key={r.kind}
            className="flex items-center gap-1 rounded bg-review-soft px-1.5 py-0.5 text-xs text-tier-review"
          >
            <AlertTriangle className="size-3 shrink-0" aria-hidden />
            {r.text}
          </li>
        ))}
        {minor.map((r) => (
          <li
            key={r.kind}
            className="rounded bg-panel-raised px-1.5 py-0.5 text-xs text-muted-foreground"
          >
            {r.text}
          </li>
        ))}
        {!serious.length && !minor.length ? (
          <li className="flex items-center gap-1 py-0.5 text-xs text-subtle-foreground">
            <ShieldCheck className="size-3 shrink-0" aria-hidden />
            No issues found
          </li>
        ) : null}
      </ul>
      <div className="flex shrink-0 items-center gap-1.5 md:justify-end">
        {file.canFixPermissions ? (
          <Button
            size="sm"
            variant="soft"
            disabled={!!busy}
            onClick={() => void change(`perm:${file.path}`, () => api.keysMakePrivate(file.path))}
          >
            {busy === `perm:${file.path}` ? <Loader2 className="animate-spin" /> : <Lock />}
            Make private
          </Button>
        ) : null}
        <Button
          size="sm"
          variant="secondary"
          onClick={() => void api.revealInFinder(file.path)}
          aria-label={`Show ${file.name} in Finder`}
        >
          <FolderOpen />
          Show
        </Button>
        <Menu>
          <MenuTrigger asChild>
            <Button size="icon" variant="ghost" aria-label={`More for ${file.name}`}>
              <MoreHorizontal />
            </Button>
          </MenuTrigger>
          <MenuContent>
            <MenuItem onSelect={() => onLabel(file)}>
              <Tag />
              {file.label ? "Edit label" : "Add label"}
            </MenuItem>
            <MenuItem onSelect={() => void navigator.clipboard.writeText(file.path)}>
              Copy path
            </MenuItem>
            <MenuSeparator />
            <MenuItem destructive onSelect={() => onTrash(file)}>
              <Trash2 />
              Move to Trash…
            </MenuItem>
          </MenuContent>
        </Menu>
      </div>
    </li>
  );
}

// ---- Page -------------------------------------------------------------------------

export function KeysPage() {
  const { report, loading, error, load, dismissError } = useKeys();
  const { summary, scanning, startScan, desktop } = useApp(
    useShallow((s) => ({
      summary: s.summary,
      scanning: s.scanning,
      startScan: s.startScan,
      desktop: s.desktop,
    })),
  );
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [labelFor, setLabelFor] = useState<KeyFile | null>(null);
  const [trashFor, setTrashFor] = useState<KeyFile | null>(null);

  // Read again after every scan: the scan is what finds the files.
  const scanId = summary?.id;
  useEffect(() => {
    void load();
  }, [load, scanId]);

  const files = useMemo(() => report?.files ?? [], [report]);
  const attention = files.filter(needsAttention).length;
  const labelled = files.filter((f) => f.label).length;
  const [howItWorks, setHowItWorks] = useState(false);
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    // Most serious first; the backend's order is kept within each group.
    const weight = (f: KeyFile) =>
      f.risks.filter((r) => r.serious).length * 10 + f.risks.filter((r) => !r.serious).length;
    const sorted = files
      .map((f, i) => [f, i] as const)
      .sort((x, y) => weight(y[0]) - weight(x[0]) || x[1] - y[1])
      .map(([f]) => f);
    return sorted.filter((f) => {
      if (filter === "attention" && !needsAttention(f)) return false;
      if (filter === "unlabelled" && f.label) return false;
      if (!q) return true;
      return [f.name, f.path, f.kindLabel, f.label?.provider ?? "", f.label?.note ?? ""].some((s) =>
        s.toLowerCase().includes(q),
      );
    });
  }, [files, query, filter]);
  const page = usePage(shown, PAGE_SIZE, `${filter}|${query}`);

  return (
    <>
      <PageHeader
        title="Keys & secrets"
        description="Private keys, certificates and credential files on this Mac, and what makes each one risky."
        actions={
          report?.scannedAtMs ? (
            <span className="text-xs text-subtle-foreground">
              From the scan {formatRelative(report.scannedAtMs)}
            </span>
          ) : null
        }
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="page-content space-y-4">
          {error ? (
            <div
              role="alert"
              className="flex items-start gap-2 rounded-lg border border-critical/40 bg-critical/10 px-3 py-2 text-[13px]"
            >
              <AlertTriangle className="mt-0.5 size-3.5 shrink-0 text-critical" />
              <p className="flex-1">{error}</p>
              <button
                type="button"
                onClick={dismissError}
                aria-label="Dismiss"
                className="rounded p-0.5 text-muted-foreground hover:bg-hover"
              >
                <X className="size-3.5" />
              </button>
            </div>
          ) : null}

          {!report ? (
            <Panel>
              <EmptyState
                icon={loading ? Loader2 : KeyRound}
                title={loading ? "Opening your list of keys…" : "Not loaded yet"}
              />
            </Panel>
          ) : !report.scanned ? (
            <Panel>
              <EmptyState
                icon={KeyRound}
                title="Scan to find your keys"
                description="The next scan looks for SSH keys, .pem and .p12 files, .env files and cloud credentials by their names. DiskPilot reads at most the first 512 bytes of each to tell a key from a certificate, and never sends anything."
                action={
                  desktop ? (
                    <Button onClick={() => void startScan()} disabled={scanning}>
                      {scanning ? <Loader2 className="animate-spin" /> : null}
                      {scanning ? "Scanning…" : "Scan now"}
                    </Button>
                  ) : undefined
                }
              />
            </Panel>
          ) : (
            <>
              <div className="flex flex-wrap items-start justify-between gap-x-8 gap-y-3">
                <div className="min-w-0">
                  <h2 className="text-[20px] leading-[26px] font-semibold tracking-tight">
                    {!files.length ? (
                      "No key files found"
                    ) : attention ? (
                      <>
                        <span className="tabular">{formatCount(attention)}</span> of{" "}
                        <span className="tabular">{formatCount(files.length)}</span> key files need
                        attention
                      </>
                    ) : (
                      <>
                        None of your <span className="tabular">{formatCount(files.length)}</span>{" "}
                        key files need attention
                      </>
                    )}
                  </h2>
                  <p className="mt-0.5 text-xs text-muted-foreground">
                    {attention
                      ? riskSummary(files)
                      : files.length
                        ? "None is readable by other accounts, left where files get shared, or in a git project without a .gitignore entry."
                        : "The last scan found no SSH or private keys, certificates, .env files or cloud credentials."}
                  </p>
                </div>
                <div className="flex max-w-[340px] items-start gap-2 pt-1 text-xs leading-relaxed text-muted-foreground">
                  <Lock className="mt-0.5 size-3.5 shrink-0 text-tier-safe" aria-hidden />
                  <p>
                    Contents are never shown or sent. Saved encrypted, with the key in your
                    Keychain.{" "}
                    <button
                      type="button"
                      aria-expanded={howItWorks}
                      onClick={() => setHowItWorks((v) => !v)}
                      className="text-accent hover:underline"
                    >
                      {howItWorks ? "Hide details" : "How it works"}
                    </button>
                  </p>
                </div>
              </div>
              {howItWorks ? (
                <p className="animate-in rounded-lg bg-panel-raised px-3 py-2.5 text-xs leading-relaxed text-muted-foreground">
                  Files are found by their names during a scan. DiskPilot reads at most the first
                  512 bytes of a file to tell a key from a certificate, never shows what is inside,
                  and never sends anything. This list and your labels are saved encrypted, with the
                  key in your Keychain. There is no export, by design.
                </p>
              ) : null}

              <div className="flex flex-wrap items-center gap-2">
                <div
                  role="group"
                  aria-label="Show"
                  className="inline-flex rounded-lg bg-panel-raised p-0.5"
                >
                  <FilterButton
                    label="All"
                    value={files.length}
                    active={filter === "all"}
                    onClick={() => setFilter("all")}
                  />
                  <FilterButton
                    label="Need attention"
                    value={attention}
                    warn={attention > 0}
                    active={filter === "attention"}
                    onClick={() => setFilter(filter === "attention" ? "all" : "attention")}
                  />
                  <FilterButton
                    label="Unlabelled"
                    value={files.length - labelled}
                    active={filter === "unlabelled"}
                    onClick={() => setFilter(filter === "unlabelled" ? "all" : "unlabelled")}
                  />
                </div>
                <label className="flex h-7 min-w-[220px] flex-1 items-center gap-2 rounded-md border border-border-strong px-2.5 sm:max-w-[380px]">
                  <Search className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
                  <input
                    type="search"
                    data-filter
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    placeholder="Search by name, folder, provider or note"
                    aria-label="Search keys"
                    className="selectable h-full min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-subtle-foreground"
                  />
                </label>
                <span className="tabular ml-auto shrink-0 text-xs text-subtle-foreground">
                  {formatCount(shown.length)} of {formatCount(files.length)}
                </span>
              </div>

              <Panel>
                <div className="hidden grid-cols-[minmax(0,1fr)_minmax(0,1.15fr)_210px] gap-x-6 border-b border-border px-3 py-1.5 text-xs text-subtle-foreground md:grid">
                  <span className="pl-10">File and where it is</span>
                  <span>Why it&apos;s risky</span>
                  <span className="w-[210px]" />
                </div>
                {shown.length ? (
                  <ul className="divide-y divide-border">
                    {page.items.map((f) => (
                      <KeyRow key={f.path} file={f} onLabel={setLabelFor} onTrash={setTrashFor} />
                    ))}
                  </ul>
                ) : (
                  <p className="px-3 py-8 text-center text-[13px] text-muted-foreground">
                    {files.length ? "Nothing matches." : "No keys or credential files found."}
                  </p>
                )}
                <Pager page={page} label="keys" className="border-t border-border px-3 py-1.5" />
              </Panel>
              <p className="text-xs leading-relaxed text-subtle-foreground">
                Dependency folders, caches, ~/Library and test fixtures are not searched. Make
                private sets a file so only your account can read it. Keys you no longer need are
                safest in the Trash, and keys you still use are safest in a password manager.
              </p>
            </>
          )}
        </div>
      </div>
      <LabelDialog file={labelFor} onClose={() => setLabelFor(null)} />
      <TrashDialog file={trashFor} onClose={() => setTrashFor(null)} />
    </>
  );
}
