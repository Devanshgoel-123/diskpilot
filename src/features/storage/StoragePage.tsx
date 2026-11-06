import { ExplorerPage } from "@/features/explorer/ExplorerPage";
import { FileKindsPage } from "@/features/file-kinds/FileKindsPage";
import { LargeFilesPage } from "@/features/large-files/LargeFilesPage";
import { useApp } from "@/lib/store";

/** Where the space went: the folder tree, the largest items, or files by type. */
export function StoragePage() {
  const view = useApp((s) => s.storageView);
  if (view === "largest") return <LargeFilesPage />;
  if (view === "types") return <FileKindsPage />;
  return <ExplorerPage />;
}
