import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn() }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
const openUrl = vi.fn();
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: (u: string) => openUrl(u) }));

import { UpdatesSettings } from "@/features/settings/UpdatesSettings";
import { useApp } from "@/lib/store";
import { useUpdates } from "@/lib/updates";

import { UpdateBanner } from "./UpdateBanner";

const initial = useUpdates.getState();
const install = vi.fn(async () => {});
const checkNow = vi.fn(async () => {});

function offer(body = "Intro.\n\n## Download\n\nGet it.\n\n## What's changed\n\n- Faster scans.") {
  return { version: "0.6.1", body } as unknown as NonNullable<typeof initial.update>;
}

beforeEach(() => {
  install.mockClear();
  checkNow.mockClear();
  useUpdates.setState({ ...initial, install, checkNow }, true);
});

describe("UpdateBanner", () => {
  it("shows nothing when DiskPilot is up to date", () => {
    const { container } = render(<UpdateBanner />);
    expect(container).toBeEmptyDOMElement();
  });

  it("offers the new version and installs it on request", () => {
    useUpdates.setState({ update: offer() });
    render(<UpdateBanner />);
    expect(screen.getByText("DiskPilot 0.6.1 is available")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Update and restart" }));
    expect(install).toHaveBeenCalledTimes(1);
  });

  it("shows download progress and hides the buttons while installing", () => {
    useUpdates.setState({
      update: offer(),
      installing: true,
      progress: { done: 25, total: 100 },
    });
    render(<UpdateBanner />);
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "25");
    expect(screen.getByText("Downloading…")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Update and restart" })).toBeNull();
  });

  it("says it is installing once the download is complete", () => {
    useUpdates.setState({ update: offer(), installing: true, progress: { done: 100, total: 100 } });
    render(<UpdateBanner />);
    expect(screen.getByText("Installing, DiskPilot will restart…")).toBeInTheDocument();
  });

  it("leaves the progress open when the size is unknown", () => {
    useUpdates.setState({ update: offer(), installing: true, progress: { done: 5, total: null } });
    render(<UpdateBanner />);
    expect(screen.getByRole("progressbar")).not.toHaveAttribute("aria-valuenow");
  });

  it("lets a failed download be retried", () => {
    useUpdates.setState({ update: offer(), installError: "Could not download the update." });
    render(<UpdateBanner />);
    expect(screen.getByText("Could not download the update.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Update and restart" })).toBeEnabled();
  });

  it("offers the download instead after an update fails verification", () => {
    useUpdates.setState({
      update: offer(),
      unverified: true,
      installError: "This update could not be verified.",
    });
    render(<UpdateBanner />);
    expect(screen.queryByRole("button", { name: "Update and restart" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Download from GitHub" }));
    expect(openUrl).toHaveBeenCalledWith(
      "https://github.com/Devanshgoel-123/diskpilot/releases/latest",
    );
    expect(install).not.toHaveBeenCalled();
  });

  it("says to move the app first when it runs from the disk image", () => {
    const env = useApp.getState().env;
    useApp.setState({
      env: {
        homeDir: "/Users/a",
        os: "macos",
        arch: "aarch64",
        appVersion: "0.6.0",
        readOnly: false,
        updateBlocked:
          "To update, quit DiskPilot, drag it into Applications, and open it from there.",
      },
    });
    useUpdates.setState({ update: offer() });
    render(<UpdateBanner />);
    expect(screen.getByText(/drag it into Applications/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Update and restart" })).toBeNull();
    useApp.setState({ env });
  });

  it("shows the notes without download steps in What's new", () => {
    useUpdates.setState({ update: offer() });
    render(<UpdateBanner />);
    fireEvent.click(screen.getByRole("button", { name: "What's new" }));
    expect(screen.getByText("What's new in DiskPilot 0.6.1")).toBeInTheDocument();
    expect(screen.getByText("Faster scans.")).toBeInTheDocument();
    expect(screen.queryByText("Get it.")).toBeNull();
  });
});

describe("Settings > Updates", () => {
  it("checks when asked", () => {
    render(<UpdatesSettings />);
    fireEvent.click(screen.getByRole("button", { name: /Check for updates/ }));
    expect(checkNow).toHaveBeenCalledWith(true);
  });

  it("turns automatic checks off", () => {
    render(<UpdatesSettings />);
    const toggle = screen.getByRole("switch", { name: "Check for updates automatically" });
    expect(toggle).toBeChecked();
    fireEvent.click(toggle);
    expect(useUpdates.getState().autoCheck).toBe(false);
  });

  it.each([
    [{ checking: true }, "Checking…"],
    [{ error: "offline" }, "Could not check: offline"],
    [{ update: offer() }, "DiskPilot 0.6.1 is available. Install it from the sidebar."],
    [{ upToDate: true }, "You have the latest version"],
  ])("explains the state %o", (state, text) => {
    useUpdates.setState(state as Partial<typeof initial>);
    render(<UpdatesSettings />);
    expect(screen.getByText((content) => content.startsWith(text))).toBeInTheDocument();
  });

  it("cannot start a second check while one is running", () => {
    useUpdates.setState({ checking: true });
    render(<UpdatesSettings />);
    expect(screen.getByRole("button", { name: /Check for updates/ })).toBeDisabled();
  });
});
