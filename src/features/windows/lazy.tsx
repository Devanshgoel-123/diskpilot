import { lazy, Suspense, type ComponentProps, type ComponentType } from "react";

import { isWindows } from "@/lib/platform";

/**
 * A Windows-only component, loaded on first use. On macOS it renders nothing
 * and its code is never downloaded, so it adds nothing to startup and never
 * subscribes to the store.
 */
function windowsOnly<P extends object>(load: () => Promise<ComponentType<P>>) {
  const Lazy = lazy(() => load().then((C) => ({ default: C })));
  return function WindowsOnly(props: P) {
    if (!isWindows()) return null;
    return (
      <Suspense fallback={null}>
        <Lazy {...props} />
      </Suspense>
    );
  };
}

export const DrivePicker = windowsOnly(() =>
  import("@/components/shared/DrivePicker").then((m) => m.DrivePicker),
);

export const DriveList = windowsOnly(() =>
  import("@/features/overview/DriveList").then((m) => m.DriveList),
);

export const FastScanOffer = windowsOnly<
  ComponentProps<typeof import("@/components/layout/FastScanDialog").FastScanOffer>
>(() => import("@/components/layout/FastScanDialog").then((m) => m.FastScanOffer));
