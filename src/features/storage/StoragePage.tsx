import { ExplorerPage } from "@/features/explorer/ExplorerPage";
import { LargeFilesPage } from "@/features/large-files/LargeFilesPage";
import { useApp } from "@/lib/store";

/** Where the space went: the folder tree, or the largest files and folders. */
export function StoragePage() {
  const view = useApp((s) => s.storageView);
  return view === "largest" ? <LargeFilesPage /> : <ExplorerPage />;
}
