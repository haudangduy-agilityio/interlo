"use client";
import { useRef, useEffect } from "react";

export interface TranslationLatencyEntry {
  id: number;
  latencyMs: number;
  inputChars: number;
  splitType: "merge" | "p-split" | "s-split";
  timestamp: number;
}

const formatLatency = (ms: number) => {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
};

const getLatencyColor = (ms: number) => {
  if (ms < 500) return "#4ade80"; // green
  if (ms < 1000) return "#facc15"; // yellow
  return "#f87171"; // red
};

const getSplitBadge = (type: string) => {
  if (type === "p-split") return { label: "paragraph", color: "#818cf8" }; // indigo
  if (type === "s-split") return { label: "speaker", color: "#38bdf8" };   // sky blue
  return null;
};

const Graph = ({ entries, itemLabel }: { entries: TranslationLatencyEntry[]; itemLabel: string }) => {
  const width = 320;
  const height = 100;
  const padding = { top: 8, right: 8, bottom: 18, left: 36 };
  const chartW = width - padding.left - padding.right;
  const chartH = height - padding.top - padding.bottom;

  if (entries.length === 0) return null;

  const maxLatency = Math.max(...entries.map((e) => e.latencyMs), 500);
  const barWidth = Math.max(2, Math.min(12, chartW / entries.length - 1));

  // Only show last N entries that fit
  const maxBars = Math.floor(chartW / (barWidth + 1));
  const visible = entries.slice(-maxBars);

  // Y-axis ticks
  const yTicks = [0, Math.round(maxLatency / 2), maxLatency];

  return (
    <svg
      width={width}
      height={height}
      className="block"
      style={{ background: "#111" }}
    >
      {/* Y-axis labels */}
      {yTicks.map((tick) => {
        const y = padding.top + chartH - (tick / maxLatency) * chartH;
        return (
          <g key={tick}>
            <line
              x1={padding.left}
              y1={y}
              x2={width - padding.right}
              y2={y}
              stroke="#333"
              strokeWidth={0.5}
            />
            <text
              x={padding.left - 4}
              y={y + 3}
              textAnchor="end"
              fill="#666"
              fontSize={8}
            >
              {formatLatency(tick)}
            </text>
          </g>
        );
      })}

      {/* Bars */}
      {visible.map((entry, i) => {
        const barH = Math.max(1, (entry.latencyMs / maxLatency) * chartH);
        const x = padding.left + i * (barWidth + 1);
        const y = padding.top + chartH - barH;
        const badge = getSplitBadge(entry.splitType);
        return (
          <g key={entry.id}>
            <rect
              x={x}
              y={y}
              width={barWidth}
              height={barH}
              fill={getLatencyColor(entry.latencyMs)}
              rx={1}
            >
              <title>
                #{entry.id} {formatLatency(entry.latencyMs)} ({entry.inputChars}ch) {entry.splitType}
              </title>
            </rect>
            {/* Split indicator dot at the base of the bar */}
            {badge && (
              <circle
                cx={x + barWidth / 2}
                cy={padding.top + chartH + 5}
                r={2}
                fill={badge.color}
              />
            )}
          </g>
        );
      })}

      {/* X-axis label */}
      <text
        x={padding.left + chartW / 2}
        y={height - 2}
        textAnchor="middle"
        fill="#555"
        fontSize={8}
      >
        {itemLabel} #
      </text>
    </svg>
  );
};

const TranslationLatencyMonitor = ({
  title = "Translation delay analysis",
  itemLabel = "translation",
  entries,
}: {
  title?: string;
  itemLabel?: string;
  entries: TranslationLatencyEntry[];
}) => {
  const listRef = useRef<HTMLDivElement>(null);

  // Auto-scroll list to bottom
  useEffect(() => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [entries.length]);

  if (entries.length === 0) return null;

  return (
    <div className="border-b border-border bg-[#111] text-[11px] font-mono">
      <div className="border-b border-border px-3 py-1.5 text-[10px] font-semibold uppercase tracking-[0.12em] text-gray-400">
        {title}
      </div>
      <div className="flex">
        {/* List — left side */}
        <div
          ref={listRef}
          className="flex-1 max-h-[120px] overflow-y-auto px-2 py-1 border-r border-border"
        >
          {entries.map((entry) => {
            const badge = getSplitBadge(entry.splitType);
            return (
              <div
                key={entry.id}
                className="flex justify-between items-center py-[1px]"
              >
                <span className="flex items-center gap-1.5">
                  <span className="text-gray-400">#{itemLabel} {entry.id}</span>
                  {badge && (
                    <span
                      className="px-1 rounded text-[9px] font-semibold"
                      style={{ color: badge.color, border: `1px solid ${badge.color}33` }}
                    >
                      {badge.label}
                    </span>
                  )}
                </span>
                <span className="flex items-center gap-2">
                  <span className="text-gray-600">{entry.inputChars}ch</span>
                  <span style={{ color: getLatencyColor(entry.latencyMs), minWidth: 48, textAlign: "right" }}>
                    {formatLatency(entry.latencyMs)}
                  </span>
                </span>
              </div>
            );
          })}
        </div>

        {/* Graph — right side */}
        <div className="flex-shrink-0">
          <Graph entries={entries} itemLabel={itemLabel} />
        </div>
      </div>
    </div>
  );
};

export default TranslationLatencyMonitor;
