import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setOs } from "@/lib/platform";
import type { AppFootprint } from "@/lib/types";

const { uninstallApp } = vi.hoisted(() => ({ uninstallApp: vi.fn() }));
vi.mock("@/lib/api", () => ({
  api: { uninstallApp: (id: string) => uninstallApp(id) },
  errorMessage: (e: unknown) => (e instanceof Error ? e.message : String(e)),
}));

import { appSubtitle, dataKindLabel } from "./labels";
import { UninstallDialog } from "./UninstallDialog";

function app(over: Partial<AppFootprint> = {}): AppFootprint {
  return {
    name: "7-Zip",
    bundleId: "HKLM64\\7-Zip",
    appPath: "C:\\Program Files\\7-Zip",
    appBytes: 5e6,
    dataBytes: 0,
    dataModifiedMs: null,
    locations: [],
    publisher: "Igor Pavlov",
    version: "24.08",
    uninstallCommand: '"C:\\Program Files\\7-Zip\\Uninstall.exe"',
    ...over,
  };
}

beforeEach(() => {
  uninstallApp.mockReset();
});
afterEach(() => setOs("macos"));

describe("UninstallDialog", () => {
  it("shows the uninstaller's command, starts it by id, then offers to refresh", async () => {
    uninstallApp.mockResolvedValue("uninstaller");
    const onRefresh = vi.fn();
    const onClose = vi.fn();
    render(<UninstallDialog app={app()} onClose={onClose} onRefresh={onRefresh} />);
    expect(screen.getByText('"C:\\Program Files\\7-Zip\\Uninstall.exe"')).toBeInTheDocument();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Start uninstaller" }));
    });
    expect(uninstallApp).toHaveBeenCalledWith("HKLM64\\7-Zip");
    expect(screen.getByText("Finish in the uninstaller")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Refresh list" }));
    expect(onRefresh).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("opens Settings when there is no uninstaller to run", async () => {
    uninstallApp.mockResolvedValue("settings");
    render(
      <UninstallDialog
        app={app({ uninstallCommand: null })}
        onClose={() => {}}
        onRefresh={() => {}}
      />,
    );
    expect(screen.getByText(/Settings > Apps > Installed apps/)).toBeInTheDocument();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Open Settings" }));
    });
    expect(screen.getByText("Finish in Settings")).toBeInTheDocument();
  });

  it("shows the engine's error and stays open", async () => {
    uninstallApp.mockRejectedValue(new Error("This program is no longer installed."));
    render(<UninstallDialog app={app()} onClose={() => {}} onRefresh={() => {}} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Start uninstaller" }));
    });
    await screen.findByText("This program is no longer installed.");
    expect(screen.getByRole("button", { name: "Start uninstaller" })).toBeEnabled();
  });
});

describe("app labels", () => {
  it("keep the macOS bundle id and Library kinds as they are", () => {
    setOs("macos");
    expect(appSubtitle(app({ bundleId: "com.example.app" }))).toBe("com.example.app");
    expect(dataKindLabel("Caches")).toBe("Caches");
    expect(dataKindLabel("Local")).toBe("Local");
  });

  it("show the publisher, version and AppData folders on Windows", () => {
    setOs("windows");
    expect(appSubtitle(app())).toBe("Igor Pavlov · 24.08");
    expect(appSubtitle(app({ publisher: null }))).toBe("24.08");
    expect(dataKindLabel("Roaming")).toBe("AppData\\Roaming");
    expect(dataKindLabel("Packages")).toBe("Store app data");
  });
});
