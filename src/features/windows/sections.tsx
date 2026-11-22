import { isWindows } from "@/lib/platform";

import { WINDOWS_SECTIONS, type WindowsSectionPage } from "./registry";

/** The Windows-only sections registered for `page` (see registry.ts). Nothing on macOS. */
export function WindowsSections({ page }: { page: WindowsSectionPage }) {
  if (!isWindows()) return null;
  const sections = WINDOWS_SECTIONS[page];
  if (!sections.length) return null;
  return (
    <>
      {sections.map((Section, i) => (
        <Section key={i} />
      ))}
    </>
  );
}
