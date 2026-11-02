import * as React from "react";

import { cn } from "@/lib/utils";

export const Input = React.forwardRef<
  HTMLInputElement,
  React.InputHTMLAttributes<HTMLInputElement>
>(({ className, ...props }, ref) => (
  <input
    ref={ref}
    className={cn(
      "selectable h-7 w-full rounded-md border border-border-strong bg-panel px-2.5 text-[13px] text-foreground placeholder:text-subtle-foreground focus-visible:border-accent focus-visible:outline-none",
      className,
    )}
    {...props}
  />
));
Input.displayName = "Input";
