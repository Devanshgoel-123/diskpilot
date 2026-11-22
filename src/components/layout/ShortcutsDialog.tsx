import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Keys } from "@/components/shared/Kbd";
import { SHORTCUT_GROUPS, actionText, keysText } from "@/lib/shortcuts";

/** Every keyboard shortcut, from the same list the website uses. */
export function ShortcutsDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="w-[min(560px,calc(100vw-32px))]">
        <DialogHeader>
          <DialogTitle>Keyboard shortcuts</DialogTitle>
          <DialogDescription>Press {keysText(["⌘", "/"])} to show this again.</DialogDescription>
        </DialogHeader>
        <div className="grid gap-x-8 gap-y-5 overflow-y-auto px-5 py-4 sm:grid-cols-2">
          {SHORTCUT_GROUPS.map((g) => (
            <section key={g.title} aria-label={g.title}>
              <h3 className="mb-1.5 text-2xs font-semibold text-subtle-foreground">{g.title}</h3>
              <dl className="space-y-1">
                {g.items.map((s) => (
                  <div key={s.action} className="flex items-center justify-between gap-3 text-xs">
                    <dt className="text-foreground/90">{actionText(s.action)}</dt>
                    <dd>
                      <Keys keys={s.keys} />
                    </dd>
                  </div>
                ))}
              </dl>
            </section>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}
