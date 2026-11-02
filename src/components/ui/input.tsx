import * as React from "react";

import { cn } from "@/lib/utils";

export const Input = React.forwardRef<
  HTMLInputElement,
  React.InputHTMLAttributes<HTMLInputElement>
>(({ className, ...props }, ref) => (
  <input
    ref={ref}
    className={cn(
      "selectable border-border-strong bg-panel text-foreground placeholder:text-subtle-foreground focus-visible:border-accent h-7 w-full rounded-md border px-2.5 text-[13px] focus-visible:outline-none",
      className,
    )}
    {...props}
  />
));
Input.displayName = "Input";
