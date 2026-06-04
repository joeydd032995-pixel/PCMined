// Display formatting helpers (hashrate, difficulty, durations).

const HASH_UNITS = ["H/s", "kH/s", "MH/s", "GH/s", "TH/s", "PH/s", "EH/s", "ZH/s"];

/** Format a raw hashes-per-second value with an appropriate SI unit. */
export function formatHashrate(hs: number): string {
  if (!isFinite(hs) || hs <= 0) return "0 H/s";
  const i = Math.min(Math.floor(Math.log10(hs) / 3), HASH_UNITS.length - 1);
  const scaled = hs / Math.pow(1000, i);
  return `${scaled.toFixed(scaled < 10 ? 2 : 1)} ${HASH_UNITS[i]}`;
}

/** Compact large numbers (difficulty, share counts). */
export function formatCompact(n: number): string {
  if (!isFinite(n)) return "—";
  return new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 2 }).format(n);
}

/** Humanize a number of seconds into a coarse duration string. */
export function formatDuration(secs: number): string {
  if (!isFinite(secs) || secs < 0) return "—";
  const units: [number, string][] = [
    [31_536_000, "y"],
    [86_400, "d"],
    [3_600, "h"],
    [60, "m"],
    [1, "s"],
  ];
  for (const [size, label] of units) {
    if (secs >= size) return `${(secs / size).toFixed(secs / size < 10 ? 1 : 0)}${label}`;
  }
  return "0s";
}
