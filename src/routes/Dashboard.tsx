import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-shell";
import {
  listCoins,
  resolveDefaultMiner,
  getNetworkStats,
  reportBlockFound,
  type CoinInfo,
  type MinerInfo,
  type NetworkStats,
} from "../lib/ipc";
import { onStats, type MinerStats } from "../lib/events";
import { LotteryBar } from "../components/LotteryBar";
import { HashrateChart } from "../components/HashrateChart";
import { MinerCard } from "../components/MinerCard";
import { expectedTimeToBlock, isBlockFound } from "../lib/lottery";
import { formatHashrate, formatCompact, formatDuration } from "../lib/format";

const NETWORK_REFRESH_MS = 30_000;
const MAX_SAMPLES = 120;

export function Dashboard() {
  const [coins, setCoins] = useState<CoinInfo[]>([]);
  const [selected, setSelected] = useState<string>("btc");
  const [net, setNet] = useState<NetworkStats | null>(null);
  const [miner, setMiner] = useState<MinerInfo | null>(null);
  const [stats, setStats] = useState<MinerStats | null>(null);
  const [samples, setSamples] = useState<[number, number][]>([]);
  const seenRef = useRef(false);
  const blockFiredRef = useRef(false);

  // Load the coin list once.
  useEffect(() => {
    listCoins().then(setCoins).catch(() => setCoins([]));
  }, []);

  // When the selected coin changes: resolve its default miner and refresh
  // network stats (then poll on an interval).
  useEffect(() => {
    let active = true;
    setStats(null);
    setSamples([]);
    seenRef.current = false;
    blockFiredRef.current = false;
    resolveDefaultMiner(selected).then((m) => active && setMiner(m)).catch(() => {});

    const refresh = () =>
      getNetworkStats(selected)
        .then((n) => active && setNet(n))
        .catch(() => {});
    refresh();
    const id = setInterval(refresh, NETWORK_REFRESH_MS);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, [selected]);

  // Live miner stats → update the card, lottery bar, and rolling chart.
  useEffect(() => {
    const unlisten = onStats((e) => {
      setStats(e.stats);
      setSamples((prev) => {
        const next: [number, number][] = [...prev, [Date.now() / 1000, e.stats.hashrate_hs]];
        return next.slice(-MAX_SAMPLES);
      });
      seenRef.current = true;
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  const yourHashrate = stats?.hashrate_hs ?? 0;
  const bestShare = stats?.best_share_diff ?? 0;

  const expectedSecs = useMemo(() => {
    if (!net) return Infinity;
    return expectedTimeToBlock(net.network_hashrate, yourHashrate, net.block_time);
  }, [net, yourHashrate]);

  // Fire the block-found alert once when a live best-share reaches difficulty.
  useEffect(() => {
    if (net && !blockFiredRef.current && isBlockFound(bestShare, net.difficulty)) {
      blockFiredRef.current = true;
      void reportBlockFound(selected, bestShare, net.difficulty);
    }
  }, [bestShare, net, selected]);

  const coin = coins.find((c) => c.id === selected);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      {/* coin selector */}
      <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
        {coins.map((c) => (
          <button
            key={c.id}
            onClick={() => setSelected(c.id)}
            style={{
              padding: "5px 12px",
              borderRadius: 6,
              border: "1px solid var(--border)",
              background: c.id === selected ? "var(--bg-3)" : "var(--bg-1)",
              color: c.id === selected ? "var(--accent)" : "var(--text-1)",
              cursor: "pointer",
              fontSize: 13,
            }}
          >
            {c.name}
            {c.monitor_only ? " ·mon" : ""}
          </button>
        ))}
      </div>

      {/* suggested wallet for receiving this coin */}
      {coin && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 10,
            fontSize: 12,
            color: "var(--text-1)",
            background: "var(--bg-1)",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            padding: "8px 12px",
          }}
        >
          <span style={{ color: "var(--text-2)" }}>Suggested wallet:</span>
          <strong>{coin.wallet_name}</strong>
          <button
            onClick={() => void open(coin.wallet_url)}
            style={{
              marginLeft: "auto",
              padding: "3px 10px",
              borderRadius: 5,
              border: "1px solid var(--border)",
              background: "var(--bg-2)",
              color: "var(--accent)",
              cursor: "pointer",
              fontSize: 12,
            }}
          >
            Install / open ↗
          </button>
          <span style={{ color: "var(--text-2)", flexBasis: "100%", fontSize: 10 }}>
            Paste your receiving address into a profile — it's checksum-validated before use. (A
            desktop app can't connect to a browser-extension wallet directly.)
          </span>
        </div>
      )}

      {/* network + odds */}
      <section
        style={{
          background: "var(--bg-1)",
          border: "1px solid var(--border)",
          borderRadius: "var(--radius)",
          padding: 16,
          display: "flex",
          flexDirection: "column",
          gap: 14,
        }}
      >
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
          <h2 style={{ margin: 0, fontSize: 15 }}>
            {coin?.name ?? selected.toUpperCase()} lottery
          </h2>
          {net?.stale && (
            <span style={{ fontSize: 11, color: "var(--warn)" }}>⚠ stale / offline data</span>
          )}
        </div>

        <div
          className="mono"
          style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: 10, fontSize: 12 }}
        >
          <Metric label="network diff" value={net ? formatCompact(net.difficulty) : "…"} />
          <Metric label="network hashrate" value={net ? formatHashrate(net.network_hashrate) : "…"} />
          <Metric label="block reward" value={net ? `${net.block_reward}` : "…"} />
          <Metric
            label="E[time to block]"
            value={isFinite(expectedSecs) ? formatDuration(expectedSecs) : "∞ (idle)"}
            hint="at your current hashrate"
          />
        </div>

        <LotteryBar
          bestShareDiff={bestShare}
          networkDifficulty={net?.difficulty ?? 0}
          stale={net?.stale}
        />

        <HashrateChart samples={samples} />
      </section>

      {/* miner */}
      {miner && <MinerCard info={miner} stats={stats} />}
    </div>
  );
}

function Metric({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div>
      <div style={{ color: "var(--text-2)", fontSize: 10 }}>{label}</div>
      <div style={{ color: "var(--text-0)", fontSize: 15 }}>{value}</div>
      {hint && <div style={{ color: "var(--text-2)", fontSize: 9 }}>{hint}</div>}
    </div>
  );
}
