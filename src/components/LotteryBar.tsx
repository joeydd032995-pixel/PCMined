// Log-scale visualization of "how close is the best share to a block?"
// The gap usually spans 10+ orders of magnitude, so a linear bar would always
// read as empty — the log scale makes incremental luck visible and honest.
import { logScalePosition, isBlockFound, LOG_DECADES } from "../lib/lottery";
import { formatCompact } from "../lib/format";

export function LotteryBar({
  bestShareDiff,
  networkDifficulty,
  stale,
}: {
  bestShareDiff: number;
  networkDifficulty: number;
  stale?: boolean;
}) {
  const pos = logScalePosition(bestShareDiff, networkDifficulty);
  const found = isBlockFound(bestShareDiff, networkDifficulty);
  const ticks = Array.from({ length: LOG_DECADES + 1 }, (_, i) => i / LOG_DECADES);

  return (
    <div className="mono" style={{ width: "100%" }}>
      <div style={{ display: "flex", justifyContent: "space-between", fontSize: 11, color: "var(--text-2)" }}>
        <span>best share: {formatCompact(bestShareDiff)}</span>
        <span>
          network diff: {stale ? "—" : formatCompact(networkDifficulty)}
          {stale ? " (stale)" : ""}
        </span>
      </div>
      <div
        style={{
          position: "relative",
          height: 18,
          marginTop: 6,
          background: "var(--bg-3)",
          borderRadius: 4,
          overflow: "hidden",
        }}
      >
        {/* decade tick marks */}
        {ticks.map((t) => (
          <div
            key={t}
            style={{
              position: "absolute",
              left: `${t * 100}%`,
              top: 0,
              bottom: 0,
              width: 1,
              background: "rgba(255,255,255,0.05)",
            }}
          />
        ))}
        <div
          style={{
            position: "absolute",
            left: 0,
            top: 0,
            bottom: 0,
            width: `${pos * 100}%`,
            background: found
              ? "var(--accent)"
              : "linear-gradient(90deg, var(--accent-dim), var(--accent))",
            transition: "width 400ms ease",
          }}
        />
        {found && (
          <div
            style={{
              position: "absolute",
              inset: 0,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              fontSize: 11,
              fontWeight: 700,
              color: "#04140a",
            }}
          >
            🎉 BLOCK FOUND
          </div>
        )}
      </div>
      <div style={{ fontSize: 10, color: "var(--text-2)", marginTop: 4 }}>
        log scale · each segment = 10× closer to a block
      </div>
    </div>
  );
}
