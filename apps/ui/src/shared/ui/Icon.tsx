// 24-unit line icons drawn at 14 to 16 px, stroke 1.8, round caps and joins, in currentColor.
const PATHS = {
  home: "M3 10.5 12 3l9 7.5V20a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z",
  live: "M4 12v0M8 8v8M12 4v16M16 8v8M20 12v0",
  doc: "M7 3h7l5 5v13H7zM14 3v5h5",
  setup:
    "M4 7h9M17 7h3M4 17h3M11 17h9M17 7a2 2 0 1 1-4 0 2 2 0 0 1 4 0zM11 17a2 2 0 1 1-4 0 2 2 0 0 1 4 0z",
  folder: "M3 6a1 1 0 0 1 1-1h5l2 2h9a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z",
  search: "M17 11a6 6 0 1 1-12 0 6 6 0 0 1 12 0zM20 20l-3.8-3.8",
  swap: "M7 7h12l-3-3M17 17H5l3 3",
  play: "M8 5v14l11-7z",
  stop: "M8 6h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z",
  chev: "m6 9 6 6 6-6",
  panel: "M5.5 5h13A2 2 0 0 1 20.5 7v10a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2zM9.5 5v14",
  shield: "M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z",
  check: "m5 12 5 5L20 7",
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 16 }: { name: IconName; size?: number }) {
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={PATHS[name]} />
    </svg>
  );
}
