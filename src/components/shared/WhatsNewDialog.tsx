import { Keyboard } from "lucide-react";
import { type ReactNode, useMemo, useRef } from "react";

import { Keys } from "@/components/shared/Kbd";
import { Markdown } from "@/components/shared/Markdown";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { splitNotes, withoutSection } from "@/lib/releaseNotes";
import { type Shortcut, actionText, shortcutsNewIn, useShortcutsDialog } from "@/lib/shortcuts";

/** Pages ⌘1 to ⌘8 as one row instead of eight. */
function compact(list: Shortcut[]): { keys: string[]; action: string }[] {
  const pages = list.filter((s) => s.page && /^\d$/.test(s.keys[1] ?? ""));
  const rest = list.filter((s) => !pages.includes(s));
  if (pages.length < 3) return list;
  const first = pages[0]?.keys[1] ?? "";
  const last = pages[pages.length - 1]?.keys[1] ?? "";
  return [{ keys: ["⌘", `${first}–${last}`], action: "Go to a page in the sidebar" }, ...rest];
}

function NewShortcuts({ list, onShowAll }: { list: Shortcut[]; onShowAll: () => void }) {
  return (
    <section
      aria-label="New keyboard shortcuts"
      className="rounded-lg border border-border bg-panel-raised p-3.5"
    >
      <div className="mb-2.5 flex items-center justify-between gap-3">
        <h3 className="flex items-center gap-2 text-xs font-semibold">
          <Keyboard className="size-3.5 text-accent" aria-hidden />
          New keyboard shortcuts
        </h3>
        <Button variant="ghost" size="sm" className="-my-1 h-7 text-xs" onClick={onShowAll}>
          See all <Keys keys={["⌘", "/"]} className="ml-1" />
        </Button>
      </div>
      <dl className="grid gap-x-6 gap-y-1.5 sm:grid-cols-2">
        {compact(list.filter((s) => s.command !== "help")).map((s) => (
          <div key={s.action} className="flex items-center justify-between gap-3 text-xs">
            <dt className="text-foreground/85">{actionText(s.action)}</dt>
            <dd>
              <Keys keys={s.keys} />
            </dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

/** Release notes for one version, with an optional action such as Update. */
export function WhatsNewDialog({
  open,
  title,
  subtitle,
  notes,
  version,
  onClose,
  footer,
}: {
  open: boolean;
  title: string;
  subtitle?: string;
  notes: string | null;
  /** The version the notes are for; its new shortcuts get their own panel. */
  version?: string;
  onClose: () => void;
  footer?: ReactNode;
}) {
  const body = useRef<HTMLDivElement>(null);
  const shortcuts = useMemo(() => shortcutsNewIn(version), [version]);
  const { lead, rest } = useMemo(() => {
    if (!notes) return { lead: null, rest: "" };
    return splitNotes(shortcuts.length ? withoutSection(notes, "Keyboard shortcuts") : notes);
  }, [notes, shortcuts.length]);

  const showAll = () => {
    onClose();
    useShortcutsDialog.getState().setOpen(true);
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent
        className="w-[min(560px,calc(100vw-32px))]"
        // Focus the notes rather than the first button, so the dialog opens
        // without a focus ring and the arrow keys scroll.
        onOpenAutoFocus={(e) => {
          e.preventDefault();
          body.current?.focus();
        }}
      >
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          {subtitle ? <DialogDescription>{subtitle}</DialogDescription> : null}
        </DialogHeader>
        <div
          ref={body}
          tabIndex={-1}
          className="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 pt-4 pb-5 focus:outline-none"
        >
          {notes ? (
            <>
              {lead ? (
                <p className="selectable text-sm leading-relaxed text-foreground">{lead}</p>
              ) : null}
              {shortcuts.length ? <NewShortcuts list={shortcuts} onShowAll={showAll} /> : null}
              {rest ? <Markdown text={rest} variant="notes" /> : null}
            </>
          ) : (
            <p className="text-[13px] text-muted-foreground">
              The full list of changes is in the release notes on GitHub.
            </p>
          )}
        </div>
        {footer ? <DialogFooter>{footer}</DialogFooter> : null}
      </DialogContent>
    </Dialog>
  );
}
