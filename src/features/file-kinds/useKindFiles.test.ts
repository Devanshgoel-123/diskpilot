import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { FileEntry, ScanSummary } from "@/lib/types";

const getFileKindFiles = vi.fn();
vi.mock("@/lib/api", () => ({ api: { getFileKindFiles: (k: string) => getFileKindFiles(k) } }));

import { useKindFiles } from "./useKindFiles";

const file = (path: string) => ({ path, name: path, bytes: 1 }) as FileEntry;
const summary = (id: string) => ({ id }) as ScanSummary;

describe("useKindFiles", () => {
  beforeEach(() => {
    getFileKindFiles.mockReset();
    getFileKindFiles.mockImplementation(async (k: string) => [file(`/${k}/a`)]);
  });

  it("fetches a type's files once and keeps the types already shown", async () => {
    const s = summary("1");
    const { result, rerender } = renderHook(({ kind }) => useKindFiles(s, kind), {
      initialProps: { kind: "videos" as "videos" | "images" },
    });
    await waitFor(() => expect(result.current.get("videos")).toHaveLength(1));
    rerender({ kind: "images" });
    await waitFor(() => expect(result.current.get("images")).toHaveLength(1));
    expect(result.current.get("videos")).toHaveLength(1);
    rerender({ kind: "videos" });
    expect(getFileKindFiles).toHaveBeenCalledTimes(2);
  });

  it("starts over for a new scan", async () => {
    const { result, rerender } = renderHook(({ s }) => useKindFiles(s, "videos"), {
      initialProps: { s: summary("1") },
    });
    await waitFor(() => expect(result.current.get("videos")).toHaveLength(1));
    rerender({ s: summary("2") });
    expect(result.current.get("videos")).toBeUndefined();
    await waitFor(() => expect(result.current.get("videos")).toHaveLength(1));
    expect(getFileKindFiles).toHaveBeenCalledTimes(2);
  });
});
