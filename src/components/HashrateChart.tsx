// Lightweight 24/7-friendly hashrate chart backed by uPlot (no heavy dashboard
// kit). Keeps a rolling window of samples and redraws on update.
import { useEffect, useRef } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import { formatHashrate } from "../lib/format";

export function HashrateChart({
  samples,
  width = 520,
  height = 160,
}: {
  /** Rolling [unixSeconds, hashrateHs] samples, oldest first. */
  samples: [number, number][];
  width?: number;
  height?: number;
}) {
  const elRef = useRef<HTMLDivElement | null>(null);
  const plotRef = useRef<uPlot | null>(null);

  useEffect(() => {
    if (!elRef.current) return;
    const opts: uPlot.Options = {
      width,
      height,
      cursor: { show: false },
      legend: { show: false },
      scales: { x: { time: true } },
      axes: [
        { stroke: "var(--text-2)", grid: { stroke: "rgba(255,255,255,0.05)" } },
        {
          stroke: "var(--text-2)",
          grid: { stroke: "rgba(255,255,255,0.05)" },
          values: (_u, vals) => vals.map((v) => formatHashrate(v)),
        },
      ],
      series: [
        {},
        { stroke: "#4ade80", width: 2, fill: "rgba(74,222,128,0.12)", points: { show: false } },
      ],
    };
    const plot = new uPlot(opts, [[], []], elRef.current);
    plotRef.current = plot;
    return () => {
      plot.destroy();
      plotRef.current = null;
    };
  }, [width, height]);

  useEffect(() => {
    const xs = samples.map((s) => s[0]);
    const ys = samples.map((s) => s[1]);
    plotRef.current?.setData([xs, ys]);
  }, [samples]);

  return <div ref={elRef} />;
}
