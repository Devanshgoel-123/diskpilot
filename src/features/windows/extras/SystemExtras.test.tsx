import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setOs } from "@/lib/platform";
import type { SystemExtra } from "@/lib/types";

const { getSystemExtras, openUrl } = vi.hoisted(() => ({
  getSystemExtras: vi.fn(),
  openUrl: vi.fn(),
}));
vi.mock("@/lib/api", () => ({
  api: { getSystemExtras: () => getSystemExtras() },
  errorMessage: (e: unknown) => String(e),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: (u: string) => openUrl(u) }));

import { sizeLabel, useSystemExtras, visibleExtras } from "./extrasStore";
import { SystemExtrasSection } from "./SystemExtras";

function extra(over: Partial<SystemExtra> = {}): SystemExtra {
  return {
    id: "hibernation-file",
    label: "Hibernation file",
    paths: ["C:\\hiberfil.sys"],
    bytes: 6.4e9,
    measured: "complete",
    explanation: "Windows saves memory here when the PC hibernates.",
    sizeNote: null,
    howToReclaim: "Turn hibernation off if you never use it.",
    command: "powercfg /h off",
    checkCommand: null,
    settingsUri: null,
    needsAdmin: true,
    ...over,
  };
}

beforeEach(() => {
  useSystemExtras.setState({ extras: null, error: null });
  getSystemExtras.mockReset();
  openUrl.mockReset();
});
afterEach(() => setOs("macos"));

describe("Windows system files", () => {
  it("is not asked for or shown on macOS", () => {
    setOs("macos");
    const { container } = render(<SystemExtrasSection />);
    expect(container).toBeEmptyDOMElement();
    expect(getSystemExtras).not.toHaveBeenCalled();
  });

  it("lists areas largest first and leaves out small ones", () => {
    const shown = visibleExtras([
      extra({ id: "a", bytes: 1e9 }),
      extra({ id: "small", bytes: 50e6 }),
      extra({ id: "b", bytes: 9e9 }),
      extra({ id: "unknown", bytes: 0, measured: "unavailable" }),
    ]);
    expect(shown.map((e) => e.id)).toEqual(["b", "a", "unknown"]);
  });

  it("says when a size is only a lower bound or unknown", () => {
    expect(sizeLabel(extra({ measured: "partial", bytes: 2e9 }))).toMatch(/^At least/);
    expect(sizeLabel(extra({ measured: "unavailable", bytes: 0 }))).toBe("Size unknown");
  });

  it("shows the Windows command and opens Settings, never removing anything", async () => {
    setOs("windows");
    getSystemExtras.mockResolvedValue([
      extra(),
      extra({
        id: "previous-windows",
        label: "Previous Windows installation",
        command: null,
        needsAdmin: false,
        settingsUri: "ms-settings:storagesense",
      }),
    ]);
    render(<SystemExtrasSection />);
    fireEvent.click(await screen.findByRole("button", { name: /Hibernation file/ }));
    expect(screen.getByText("powercfg /h off")).toBeInTheDocument();
    expect(screen.getByText("Run in an administrator terminal")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Previous Windows installation/ }));
    fireEvent.click(screen.getByRole("button", { name: "Open Settings" }));
    expect(openUrl).toHaveBeenCalledWith("ms-settings:storagesense");
    expect(screen.queryByRole("button", { name: /delete|remove/i })).toBeNull();
  });
});
