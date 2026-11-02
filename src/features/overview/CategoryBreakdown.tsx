import { Bar, BarChart, LabelList, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";

import { Panel, PanelHeader } from "@/components/ui/panel";
import { CATEGORY_META } from "@/lib/categories";
import { formatBytes, formatCount, formatPercent } from "@/lib/format";
import type { CategoryTotal } from "@/lib/types";

interface Row {
  label: string;
  description: string;
  bytes: number;
  fileCount: number;
}

function ChartTooltip({
  active,
  payload,
  total,
}: {
  active?: boolean;
  payload?: { payload: Row }[];
  total: number;
}) {
  const row = payload?.[0]?.payload;
  if (!active || !row) return null;
  return (
    <div className="border-border-strong bg-panel-raised max-w-[240px] rounded-md border px-2.5 py-2 text-xs shadow-lg shadow-black/20">
      <p className="font-medium">{row.label}</p>
      <p className="text-foreground tabular mt-0.5">
        {formatBytes(row.bytes)} · {formatPercent(row.bytes, total)} · {formatCount(row.fileCount)}{" "}
        files
      </p>
      <p className="text-muted-foreground mt-1 leading-snug">{row.description}</p>
    </div>
  );
}

/**
 * Ranked, single-hue bars: the job is comparing magnitudes across eleven
 * categories, which a pie or a multi-color palette would obscure.
 */
export function CategoryBreakdown({ categories }: { categories: CategoryTotal[] }) {
  const total = categories.reduce((a, c) => a + c.bytes, 0);
  const rows: Row[] = [...categories]
    .sort((a, b) => b.bytes - a.bytes)
    .map((c) => ({
      label: CATEGORY_META[c.category].label,
      description: CATEGORY_META[c.category].description,
      bytes: c.bytes,
      fileCount: c.fileCount,
    }));

  return (
    <Panel>
      <PanelHeader
        title="Breakdown by category"
        description="Every scanned byte counted once, in the category of the rule that matched it."
      />
      <div className="px-2 pb-3" style={{ height: rows.length * 28 + 8 }}>
        <ResponsiveContainer width="100%" height="100%">
          <BarChart
            data={rows}
            layout="vertical"
            margin={{ top: 0, right: 64, bottom: 0, left: 8 }}
          >
            <XAxis type="number" hide domain={[0, "dataMax"]} />
            <YAxis
              type="category"
              dataKey="label"
              width={128}
              tickLine={false}
              axisLine={false}
              tick={{ fill: "var(--muted-foreground)", fontSize: 12 }}
            />
            <Tooltip
              cursor={{ fill: "var(--hover)" }}
              content={<ChartTooltip total={total} />}
              isAnimationActive={false}
            />
            <Bar
              dataKey="bytes"
              fill="var(--accent)"
              radius={[0, 4, 4, 0]}
              barSize={12}
              isAnimationActive={false}
            >
              <LabelList
                dataKey="bytes"
                position="right"
                formatter={(v: unknown) => formatBytes(Number(v))}
                style={{
                  fill: "var(--foreground)",
                  fontSize: 12,
                  fontVariantNumeric: "tabular-nums",
                }}
              />
            </Bar>
          </BarChart>
        </ResponsiveContainer>
      </div>
    </Panel>
  );
}
