import type { ReactNode } from "react";

import { Markdown } from "@/components/shared/Markdown";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

/** Release notes for one version, with an optional action such as Update. */
export function WhatsNewDialog({
  open,
  title,
  subtitle,
  notes,
  onClose,
  footer,
}: {
  open: boolean;
  title: string;
  subtitle?: string;
  notes: string | null;
  onClose: () => void;
  footer?: ReactNode;
}) {
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          {subtitle ? <DialogDescription>{subtitle}</DialogDescription> : null}
        </DialogHeader>
        <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-4 text-[13px]">
          {notes ? (
            <Markdown text={notes} />
          ) : (
            <p className="text-muted-foreground">
              The full list of changes is in the release notes on GitHub.
            </p>
          )}
        </div>
        {footer ? <DialogFooter>{footer}</DialogFooter> : null}
      </DialogContent>
    </Dialog>
  );
}
