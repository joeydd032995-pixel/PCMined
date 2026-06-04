// A miner card. Fee is FIRST-CLASS here: a 0% miner gets a green badge, any
// dev fee gets an amber "⚠" badge with the exact percentage. Source + license
// are always shown so the user knows exactly what they're running.
import { open } from "@tauri-apps/plugin-shell";
import type { MinerInfo } from "../lib/ipc";
import type { MinerStats } from "../lib/events";
import { formatHashrate, formatCompact } from "../lib/format";

function FeeBadge({ info }: { info: MinerInfo }) {
  const free = info.dev_fee_pct <= 0;
  return (
    <span
      title={free ? "No dev fee" : "Discloses a dev fee — confirmation required before start"}
      style={{
        fontSize: 11,
        fontWeight: 700,
        padding: "2px 8px",
        borderRadius: 999,
        color: free ? "#04140a" : "#1a1405",
        background: free ? "var(--accent)" : "var(--warn)",
      }}
    >
      {free
        ? info.id === "cpuminer-opt"
          ? "0% · slower"
          : "0% fee"
        : `~${info.dev_fee_pct}% fee ⚠`}
    </span>
  );
}

export function MinerCard({ info, stats }: { info: MinerInfo; stats?: MinerStats | null }) {
  return (
    <div
      style={{
        background: "var(--bg-1)",
        border: "1px solid var(--border)",
        borderRadius: "var(--radius)",
        padding: 14,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
        <strong>{info.display}</strong>
        <FeeBadge info={info} />
      </div>

      <div style={{ marginTop: 6, fontSize: 11, color: "var(--text-2)" }} className="mono">
        {info.open_source ? "open source" : "closed source"} · {info.license} ·{" "}
        <a
          href={info.source_url}
          onClick={(e) => {
            e.preventDefault();
            void open(info.source_url);
          }}
        >
          source
        </a>
      </div>

      {stats && (
        <div
          className="mono"
          style={{
            marginTop: 10,
            display: "grid",
            gridTemplateColumns: "1fr 1fr",
            gap: 6,
            fontSize: 12,
          }}
        >
          <Stat label="hashrate" value={formatHashrate(stats.hashrate_hs)} />
          <Stat label="best share" value={formatCompact(stats.best_share_diff)} />
          <Stat label="accepted" value={String(stats.accepted)} />
          <Stat label="rejected" value={String(stats.rejected)} />
          <Stat
            label="status"
            value={stats.connected ? "connected" : "offline"}
            color={stats.connected ? "var(--accent)" : "var(--danger)"}
          />
          <Stat
            label="source"
            value={stats.source.kind === "live" ? "live" : `stale ${stats.source.age_secs}s`}
            color={stats.source.kind === "live" ? "var(--text-1)" : "var(--warn)"}
          />
        </div>
      )}
    </div>
  );
}

function Stat({ label, value, color }: { label: string; value: string; color?: string }) {
  return (
    <div style={{ display: "flex", justifyContent: "space-between" }}>
      <span style={{ color: "var(--text-2)" }}>{label}</span>
      <span style={{ color: color ?? "var(--text-0)" }}>{value}</span>
    </div>
  );
}
