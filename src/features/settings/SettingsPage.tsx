import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, FolderOpen, Lock, Monitor, Moon, Sun } from "lucide-react";
import type * as React from "react";

import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { Panel } from "@/components/ui/panel";
import { errorMessage } from "@/lib/api";
import { tildify } from "@/lib/format";
import { useApp } from "@/lib/store";
import type { ThemePreference } from "@/lib/theme";
import { cn } from "@/lib/utils";

const REPO_URL = "https://github.com/Devanshgoel-123/diskpilot";

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <h2 className="mb-2 text-xs font-medium text-muted-foreground">{title}</h2>
      <Panel className="divide-y divide-border">{children}</Panel>
    </section>
  );
}

function Setting({
  label,
  description,
  control,
}: {
  label: string;
  description?: React.ReactNode;
  control: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-6 px-4 py-3">
      <div className="min-w-0">
        <p className="text-[13px]">{label}</p>
        {description ? (
          <div className="mt-0.5 text-xs leading-relaxed text-muted-foreground">{description}</div>
        ) : null}
      </div>
      <div className="shrink-0">{control}</div>
    </div>
  );
}

const THEMES: { id: ThemePreference; label: string; icon: typeof Sun }[] = [
  { id: "dark", label: "Dark", icon: Moon },
  { id: "light", label: "Light", icon: Sun },
  { id: "system", label: "System", icon: Monitor },
];

export function SettingsPage() {
  const { theme, setTheme, scanRoot, setScanRoot, env, scanning } = useApp();
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
          </Section>

          <Section title="Safety">
            <Setting
              label="Cleanup safety"
              description="Cleanup is always previewed with exact paths and confirmed by you. Items go to the Trash by default; permanent deletion is offered only for regenerable data. Every path is re-checked right before it is touched, protected system and home folders are refused, and symbolic links are never followed. Docker volumes are never removed."
              control={
                <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
                  <Lock className="size-3" aria-hidden />
                  Always on
                </span>
              }
            />
            <Setting
              label="Privacy"
              description="Everything runs locally. No account, telemetry or network access is needed to scan."
              control={null}
            />
          </Section>

          <Section title="About">
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
