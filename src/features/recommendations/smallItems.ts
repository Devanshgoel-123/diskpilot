/**
 * Developer Tools and AI agent leftovers below this are reviewed from those pages,
 * where every one has a checkbox, and only listed here once handed over:
 * dozens of few-KB editor workspaces would bury the items worth freeing.
 */
export const SMALL_TOOL_ITEM_BYTES = 1_048_576;

/** Whether a recommendation is one of those small Developer Tools items. */
export function isSmallToolItem(r: { source: string; bytes: number }) {
  return r.source !== "scan" && r.bytes < SMALL_TOOL_ITEM_BYTES;
}
