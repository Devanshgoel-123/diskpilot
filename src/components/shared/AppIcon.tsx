import type { ReactNode } from "react";

import { useAppBadge } from "@/lib/appBadges";
import { cn } from "@/lib/utils";

/**
 * The icon of the installed app a path belongs to, rendered from the app
 * itself. Shows `fallback` when no installed app owns the path.
 */
export function AppIcon({
  path,
  fallback,
  className,
}: {
  path: string | null | undefined;
  fallback?: ReactNode;
  className?: string;
}) {
  const badge = useAppBadge(path);
  if (!badge) return <>{fallback ?? null}</>;
  return (
    <img
      src={badge.icon}
      alt=""
      title={badge.name}
      draggable={false}
      className={cn("size-4 shrink-0 object-contain", className)}
    />
  );
}
