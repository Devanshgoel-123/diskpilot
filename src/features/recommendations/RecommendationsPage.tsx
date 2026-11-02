import { CircleCheck, CircleHelp, Lock } from "lucide-react";

import { PageHeader } from "@/components/layout/PageHeader";
import { RoadmapNote } from "@/components/shared/RoadmapNote";
import { Panel } from "@/components/ui/panel";

const TIERS = [
  {
    icon: CircleCheck,
    title: "Likely safe",
    tone: "text-good",
    text: "Well-understood, regenerable targets such as package manager caches or Xcode DerivedData. Still shown in full and confirmed by you before anything happens.",
  },
  {
    icon: CircleHelp,
    title: "Review first",
    tone: "text-warning",
    text: "Possibly disposable, but an app or project may depend on it: node_modules of active projects, old toolchains, simulator data.",
  },
  {
    icon: Lock,
    title: "Keep",
    tone: "text-muted-foreground",
    text: "Personal files, system files, persistent data like Docker volumes, and anything DiskPilot cannot explain.",
  },
];

const PIPELINE = [
  ["Scanner", "Reads sizes. Read-only."],
  ["Classifier", "Explains what each item is. Cannot act."],
  ["Recommendations", "Assigns a tier and a reason."],
  ["Planner", "Builds an exact, previewable list of paths and operations."],
  ["Safety gate", "Validates every path: inside approved roots, not protected, no symlinks."],
  ["Executor", "Runs only approved, predefined operations and records the result."],
];

export function RecommendationsPage() {
  return (
    <>
      <PageHeader
        title="Recommendations"
        description="Cleanup suggestions with reasons and risk levels."
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[920px] space-y-4 px-6 py-5">
          <RoadmapNote phase="Phase 2" title="Recommendations are not built yet">
            DiskPilot is currently read-only and cannot delete anything. This is how cleanup will
            work when it ships.
          </RoadmapNote>
          <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
            {TIERS.map((t) => (
              <Panel key={t.title} className="p-4">
                <p className="flex items-center gap-2 text-[13px] font-medium">
                  <t.icon className={`size-4 ${t.tone}`} aria-hidden />
                  {t.title}
                </p>
                <p className="text-muted-foreground mt-2 text-xs leading-relaxed">{t.text}</p>
              </Panel>
            ))}
          </div>
          <Panel className="p-4">
            <p className="text-[13px] font-medium">Every deletion passes through one pipeline</p>
            <ol className="mt-3 space-y-2">
              {PIPELINE.map(([name, text], i) => (
                <li key={name} className="flex gap-3 text-xs">
                  <span className="border-border-strong text-2xs text-muted-foreground tabular flex size-5 shrink-0 items-center justify-center rounded-full border">
                    {i + 1}
                  </span>
                  <span>
                    <span className="font-medium">{name}</span>{" "}
                    <span className="text-muted-foreground">{text}</span>
                  </span>
                </li>
              ))}
            </ol>
            <p className="text-muted-foreground mt-3 text-xs">
              Nothing is deleted without your explicit approval of the exact paths. Deleted data
              cannot always be restored.
            </p>
          </Panel>
        </div>
      </div>
    </>
  );
}
