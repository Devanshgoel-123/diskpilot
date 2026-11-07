import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Schedule } from "@/lib/types";

const getSchedule = vi.fn();
const setSchedule = vi.fn();
const sendTestReminder = vi.fn();
vi.mock("@/lib/api", () => ({
  api: {
    getSchedule: () => getSchedule(),
    setSchedule: (s: Schedule) => setSchedule(s),
    sendTestReminder: () => sendTestReminder(),
  },
  errorMessage: (e: unknown) => (e instanceof Error ? e.message : String(e)),
}));
const autostart = { on: false };
vi.mock("@tauri-apps/plugin-autostart", () => ({
  isEnabled: () => Promise.resolve(autostart.on),
  enable: () => {
    autostart.on = true;
    return Promise.resolve();
  },
  disable: () => {
    autostart.on = false;
    return Promise.resolve();
  },
}));
const permission = { granted: false, answer: "denied" };
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: () => Promise.resolve(permission.granted),
  requestPermission: () => Promise.resolve(permission.answer),
}));

import { BackgroundSettings } from "./BackgroundSettings";

const off: Schedule = { rescan: "off", reminder: false, lastReminderMs: null };

beforeEach(() => {
  getSchedule.mockReset().mockResolvedValue(off);
  setSchedule.mockReset().mockImplementation((s: Schedule) => Promise.resolve(s));
  sendTestReminder.mockReset().mockResolvedValue(null);
  autostart.on = false;
  permission.granted = false;
  permission.answer = "denied";
});

/** Rendered, with the saved settings and Open at login both loaded. */
async function renderLoaded() {
  render(<BackgroundSettings />);
  await waitFor(() => {
    expect(screen.getByRole("combobox", { name: "Rescan automatically" })).toBeEnabled();
    expect(screen.getByRole("switch", { name: "Open at login" })).toBeEnabled();
  });
}

describe("Background settings", () => {
  it("starts with everything off", async () => {
    await renderLoaded();
    expect(screen.getByRole("switch", { name: "Open at login" })).not.toBeChecked();
    expect(screen.getByRole("switch", { name: "Weekly reminder" })).not.toBeChecked();
  });

  it("does not turn the reminder on when macOS refuses notifications", async () => {
    await renderLoaded();
    await act(async () => {
      fireEvent.click(screen.getByRole("switch", { name: "Weekly reminder" }));
    });
    expect(setSchedule).not.toHaveBeenCalled();
    expect(screen.getByText(/not letting DiskPilot send notifications/)).toBeInTheDocument();
  });

  it("turns the reminder on once allowed, and can send one now", async () => {
    permission.answer = "granted";
    await renderLoaded();
    await act(async () => {
      fireEvent.click(screen.getByRole("switch", { name: "Weekly reminder" }));
    });
    expect(setSchedule).toHaveBeenCalledWith({ ...off, reminder: true });
    expect(screen.getByRole("switch", { name: "Weekly reminder" })).toBeChecked();
    // Running in the background needs Open at login to survive a restart.
    expect(screen.getByText(/Turn on Open at login/)).toBeInTheDocument();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Send one now" }));
    });
    expect(sendTestReminder).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Sent. Send another" })).toBeInTheDocument();
  });

  it("saves how often to rescan, and Open at login clears the hint", async () => {
    await renderLoaded();
    await act(async () => {
      fireEvent.change(screen.getByRole("combobox", { name: "Rescan automatically" }), {
        target: { value: "daily" },
      });
    });
    expect(setSchedule).toHaveBeenCalledWith({ ...off, rescan: "daily" });
    expect(screen.getByText(/Turn on Open at login/)).toBeInTheDocument();
    await act(async () => {
      fireEvent.click(screen.getByRole("switch", { name: "Open at login" }));
    });
    expect(autostart.on).toBe(true);
    expect(screen.queryByText(/Turn on Open at login/)).toBeNull();
  });
});
