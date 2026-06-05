import type { MinerState } from "../lib/events";

const COLORS: Record<MinerState, string> = {
  idle: "var(--text-2)",
  starting: "var(--info)",
  running: "var(--accent)",
  reconnecting: "var(--warn)",
  crashed: "var(--danger)",
  stopped: "var(--text-2)",
};

/** Small colored status indicator for a miner's lifecycle state. */
export function StatusDot({ state, title }: { state: MinerState; title?: string }) {
  return (
    <span
      title={title ?? state}
      style={{
        display: "inline-block",
        width: 9,
        height: 9,
        borderRadius: "50%",
        background: COLORS[state] ?? "var(--text-2)",
        boxShadow: state === "running" ? "0 0 6px var(--accent)" : "none",
      }}
    />
  );
}
