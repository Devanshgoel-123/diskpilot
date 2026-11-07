import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { useEffect, useState } from "react";

import { api, errorMessage } from "@/lib/api";
import type { Schedule } from "@/lib/types";

import { Section, Setting } from "./parts";

const selectClass =
  "h-7 max-w-64 rounded-md border border-border-strong bg-panel px-2.5 text-[13px] focus-visible:border-accent focus-visible:outline-none";

/** Open at login, scheduled rescans and the weekly reminder. All off by default. */
export function BackgroundSettings() {
  const [schedule, setSchedule] = useState<Schedule | null>(null);
  const [atLogin, setAtLogin] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [tested, setTested] = useState(false);

  useEffect(() => {
    void api
      .getSchedule()
      .then(setSchedule)
      .catch((e: unknown) => setError(errorMessage(e)));
    void isEnabled()
      .then(setAtLogin)
      .catch(() => setAtLogin(false));
  }, []);

  const save = async (next: Schedule) => {
    setError(null);
    try {
      setSchedule(await api.setSchedule(next));
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const toggleLogin = async (on: boolean) => {
    setError(null);
    try {
      await (on ? enable() : disable());
      setAtLogin(await isEnabled());
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  // macOS asks once whether DiskPilot may send notifications.
  const toggleReminder = async (on: boolean) => {
    if (!schedule) return;
    if (on) {
      const granted = (await isPermissionGranted()) || (await requestPermission()) === "granted";
      if (!granted) {
        setError(
          "macOS is not letting DiskPilot send notifications. Allow them in System Settings > Notifications > DiskPilot, then turn this on again.",
        );
        return;
      }
    }
    await save({ ...schedule, reminder: on });
  };

  const test = async () => {
    setTesting(true);
    setError(null);
    try {
      await api.sendTestReminder();
      setTested(true);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setTesting(false);
    }
  };

  const needsLogin = schedule && (schedule.rescan !== "off" || schedule.reminder) && !atLogin;

  return (
    <Section title="In the background" id="background">
      <Setting
        label="Open at login"
        description="Start DiskPilot in the menu bar when you log in, without opening its window."
        control={
          <input
            type="checkbox"
            role="switch"
            checked={!!atLogin}
            disabled={atLogin === null}
            onChange={(e) => void toggleLogin(e.target.checked)}
            aria-label="Open at login"
            className="size-4 accent-[var(--accent)]"
          />
        }
      />
      <Setting
        label="Rescan automatically"
        description="When the last results are older than this, DiskPilot scans the same folder again. Scanning only reads; nothing is removed."
        control={
          <select
            value={schedule?.rescan ?? "off"}
            disabled={!schedule}
            onChange={(e) =>
              schedule && void save({ ...schedule, rescan: e.target.value as Schedule["rescan"] })
            }
            aria-label="Rescan automatically"
            className={selectClass}
          >
            <option value="off">Never</option>
            <option value="daily">Every day</option>
            <option value="weekly">Every week</option>
          </select>
        }
      />
      <Setting
        label="Weekly reminder"
        description={
          <>
            Once a week, during the day, a notification says how much is likely safe to clear and
            how your cleanup streak is going. DiskPilot never cleans anything on its own.
            {schedule?.reminder ? (
              <button
                type="button"
                className="ml-1 text-accent underline-offset-2 hover:underline disabled:opacity-60"
                onClick={() => void test()}
                disabled={testing}
              >
                {testing ? "Sending…" : tested ? "Sent. Send another" : "Send one now"}
              </button>
            ) : null}
          </>
        }
        control={
          <input
            type="checkbox"
            role="switch"
            checked={!!schedule?.reminder}
            disabled={!schedule}
            onChange={(e) => void toggleReminder(e.target.checked)}
            aria-label="Weekly reminder"
            className="size-4 accent-[var(--accent)]"
          />
        }
      />
      {needsLogin || error ? (
        <div className="space-y-1 px-4 py-2.5 text-xs" aria-live="polite">
          {needsLogin ? (
            <p className="text-muted-foreground">
              These run while DiskPilot is open or in the menu bar. Turn on Open at login so they
              continue after you restart your Mac.
            </p>
          ) : null}
          {error ? <p className="text-critical">{error}</p> : null}
        </div>
      ) : null}
    </Section>
  );
}
