import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, FolderOpen, Lock, Monitor, Moon, Sun } from "lucide-react";
import { useEffect, useState } from "react";
import { useShallow } from "zustand/react/shallow";

import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { api, errorMessage } from "@/lib/api";
import { formatBytes, formatRelative, tildify } from "@/lib/format";
import { useShortcutsDialog } from "@/lib/shortcuts";
import { playScanDone, setSoundsEnabled, soundsEnabled } from "@/lib/sounds";
import { useApp } from "@/lib/store";
import type { ThemePreference } from "@/lib/theme";
import { cn } from "@/lib/utils";

import { AssistantSettings } from "./AssistantSettings";
import { BackgroundSettings } from "./BackgroundSettings";
import { UpdatesSettings } from "./UpdatesSettings";
import { Section, Setting, Switch } from "./parts";

const REPO_URL = "https://github.com/Devanshgoel-123/diskpilot";

const THEMES: { id: ThemePreference; label: string; icon: typeof Sun }[] = [
  { id: "dark", label: "Dark", icon: Moon },
  { id: "light", label: "Light", icon: Sun },
  { id: "system", label: "System", icon: Monitor },
];

function SavedResults() {
  const { summary, scanning, forgetScan } = useApp(
    useShallow((s) => ({ summary: s.summary, scanning: s.scanning, forgetScan: s.forgetScan })),
  );
  const [fileBytes, setFileBytes] = useState<number | null>(null);
  // Re-read after each scan (which saves) and after clearing.
  useEffect(() => {
    let live = true;
    api
      .getSavedScanInfo()
      .then((info) => {
        if (live) setFileBytes(info.fileBytes);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [summary]);

  return (
    <Setting
      label="Saved results"
      description={
        fileBytes != null && summary ? (
          <>
            Results from the scan {formatRelative(summary.finishedAtMs)} are kept on this Mac (
            {formatBytes(fileBytes)}) so they appear right away next time. They hold folder names
            and sizes, never file contents. A new scan replaces them.
          </>
        ) : (
          "The last scan is kept on this Mac so results appear right away next time. Nothing is saved yet."
        )
      }
      control={
        <Button
          variant="secondary"
          disabled={scanning || (fileBytes == null && !summary)}
          onClick={() => void forgetScan().then(() => setFileBytes(null))}
        >
          Clear results
        </Button>
      }
    />
  );
}

export function SettingsPage() {
  const { theme, setTheme, scanRoot, setScanRoot, env, scanning } = useApp(
    useShallow((s) => ({
      theme: s.theme,
      setTheme: s.setTheme,
      scanRoot: s.scanRoot,
      setScanRoot: s.setScanRoot,
      env: s.env,
      scanning: s.scanning,
    })),
  );
  const home = env?.homeDir;

  const chooseFolder = async () => {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        defaultPath: scanRoot ?? home,
      });
      if (typeof picked === "string") setScanRoot(picked);
    } catch (e) {
      console.error(errorMessage(e));
    }
  };

  return (
    <>
      <PageHeader title="Settings" />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[720px] space-y-6 px-6 py-5">
          <Section title="Appearance">
            <Setting
              label="Theme"
              control={
                <div
                  role="radiogroup"
                  aria-label="Theme"
                  className="flex rounded-md border border-border-strong p-0.5"
                >
                  {THEMES.map((t) => (
                    <button
                      key={t.id}
                      type="button"
                      role="radio"
                      aria-checked={theme === t.id}
                      onClick={() => setTheme(t.id)}
                      className={cn(
                        "flex h-6 items-center gap-1.5 rounded px-2.5 text-xs",
                        theme === t.id
                          ? "bg-hover font-medium text-foreground"
                          : "text-muted-foreground hover:text-foreground",
                      )}
                    >
                      <t.icon className="size-3" aria-hidden />
                      {t.label}
                    </button>
                  ))}
                </div>
              }
            />
            <SoundsSetting />
          </Section>

          <Section title="Scanning">
            <Setting
              label="Folder to scan"
              description={
                <>
                  <span className="font-mono text-foreground">
                    {tildify(scanRoot ?? home ?? "~", home)}
                  </span>
                  {scanRoot ? null : " (your home folder)"}. macOS may ask for permission to read
                  protected folders such as Desktop, Documents and Downloads.
                </>
              }
              control={
                <div className="flex items-center gap-1.5">
                  {scanRoot ? (
                    <Button variant="ghost" onClick={() => setScanRoot(null)} disabled={scanning}>
                      Reset
                    </Button>
                  ) : null}
                  <Button
                    variant="secondary"
                    onClick={() => void chooseFolder()}
                    disabled={scanning}
                  >
                    <FolderOpen />
                    Choose…
                  </Button>
                </div>
              }
            />
            <Setting
              label="Scan behavior"
              description="Symbolic links are never followed, other volumes are skipped, and hard-linked files are counted once. Sizes are space allocated on disk."
              control={null}
            />
            <SavedResults />
          </Section>

          <AssistantSettings />

          <Section title="Safety">
            <Setting
              label="Cleanup safety"
              description="Cleanup is always previewed with exact paths and confirmed by you. Items go to the Trash by default; permanent deletion is offered only for regenerable data. Every path is re-checked right before it is touched, protected system and home folders are refused, and symbolic links are never followed. A Docker volume is removed only when you choose it and confirm by typing a name."
              control={
                <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
                  <Lock className="size-3" aria-hidden />
                  Always on
                </span>
              }
            />
            <Setting
              label="Privacy"
              description="Scanning, analysis and cleanup run entirely on this Mac. There is no account and no telemetry. The optional assistant is the only feature that uses the network, and only when you ask it a question."
              control={null}
            />
          </Section>

          <BackgroundSettings />

          <UpdatesSettings />

          <Section title="About">
            <Setting
              label="Keyboard shortcuts"
              description="Go to any page with ⌘1 to ⌘8, scan again with ⌘R, filter a list with ⌘F."
              control={
                <Button
                  variant="secondary"
                  onClick={() => useShortcutsDialog.getState().setOpen(true)}
                >
                  Show all
                  <kbd className="ml-1 font-mono text-2xs text-muted-foreground">⌘/</kbd>
                </Button>
              }
            />
            <Setting
              label="DiskPilot"
              description={`Version ${env?.appVersion ?? "—"} · ${env?.os ?? ""} ${env?.arch ?? ""} · MIT License`}
              control={
                <Button variant="ghost" onClick={() => void openUrl(REPO_URL)}>
                  <ExternalLink />
                  GitHub
                </Button>
              }
            />
          </Section>
        </div>
      </div>
    </>
  );
}

function SoundsSetting() {
  const [on, setOn] = useState(soundsEnabled);
  return (
    <Setting
      label="Sounds"
      description="A quiet tone while a scan runs, and a soft chime when a scan or cleanup finishes. Never while the window is hidden."
      control={
        <Switch
          on={on}
          label="Sounds"
          onChange={(v) => {
            setSoundsEnabled(v);
            setOn(v);
            if (v) playScanDone();
          }}
        />
      }
    />
  );
}
