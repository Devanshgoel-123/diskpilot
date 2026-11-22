import { lazy, Suspense } from "react";

import { isWindows } from "@/lib/platform";

import type { WindowsSectionPage } from "./registry";

// Loaded on first use, and only on Windows, so macOS never downloads it.
const Registered = lazy(() =>
  import("./registry").then((m) => ({
    default: function Registered({ page }: { page: WindowsSectionPage }) {
      return (
        <>
          {m.WINDOWS_SECTIONS[page].map((Section, i) => (
            <Section key={i} />
          ))}
        </>
      );
    },
  })),
);

/** The Windows-only sections registered for `page` (see registry.ts). Nothing on macOS. */
export function WindowsSections({ page }: { page: WindowsSectionPage }) {
  if (!isWindows()) return null;
  return (
    <Suspense fallback={null}>
      <Registered page={page} />
    </Suspense>
  );
}
