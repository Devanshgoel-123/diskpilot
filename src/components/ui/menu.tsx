import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import * as React from "react";

import { cn } from "@/lib/utils";

export const Menu = DropdownMenu.Root;
export const MenuTrigger = DropdownMenu.Trigger;

export const MenuContent = React.forwardRef<
  React.ElementRef<typeof DropdownMenu.Content>,
  React.ComponentPropsWithoutRef<typeof DropdownMenu.Content>
>(({ className, sideOffset = 4, align = "end", ...props }, ref) => (
  <DropdownMenu.Portal>
    <DropdownMenu.Content
      ref={ref}
      sideOffset={sideOffset}
      align={align}
      className={cn(
        "animate-in z-50 min-w-[180px] rounded-lg bg-panel-raised p-1 text-[13px] shadow-float",
        className,
      )}
      {...props}
    />
  </DropdownMenu.Portal>
));
MenuContent.displayName = "MenuContent";

/**
 * A menu command. `destructive` items are red and should open a
 * confirmation, never act directly. A disabled item can explain why.
 */
export const MenuItem = React.forwardRef<
  React.ElementRef<typeof DropdownMenu.Item>,
  React.ComponentPropsWithoutRef<typeof DropdownMenu.Item> & {
    destructive?: boolean;
    hint?: string | null;
  }
>(({ className, destructive, hint, children, ...props }, ref) => (
  <DropdownMenu.Item
    ref={ref}
    className={cn(
      "flex cursor-default flex-col rounded-md px-2 py-1.5 outline-none select-none data-[disabled]:opacity-50 data-[highlighted]:bg-hover [&_svg]:size-3.5",
      destructive && "text-critical",
      className,
    )}
    {...props}
  >
    <span className="flex items-center gap-2">{children}</span>
    {hint ? <span className="mt-0.5 text-xs text-muted-foreground">{hint}</span> : null}
  </DropdownMenu.Item>
));
MenuItem.displayName = "MenuItem";

export function MenuSeparator() {
  return <DropdownMenu.Separator className="my-1 h-px bg-border" />;
}
