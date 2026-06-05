import { useEffect, useRef, useState } from "react";
import { onLog, type LogEvent } from "../lib/events";

const MAX_LINES = 2000;
// Lines containing these are hidden unless "verbose" is on.
const NOISE = /\b(debug|trace|ping)\b/i;

/** Live log viewer: subscribes to miner://log, with a verbose toggle and export. */
export function LogViewer() {
  const [lines, setLines] = useState<{ id: string; line: string }[]>([]);
  const [verbose, setVerbose] = useState(false);
  const [autoscroll, setAutoscroll] = useState(true);
  const endRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const unlisten = onLog((e: LogEvent) => {
      setLines((prev) => {
        const next = [...prev, { id: e.miner_id, line: e.line }];
        return next.slice(-MAX_LINES);
      });
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (autoscroll) endRef.current?.scrollIntoView({ block: "end" });
  }, [lines, autoscroll]);

  const shown = verbose ? lines : lines.filter((l) => !NOISE.test(l.line));

  const exportLogs = () => {
    const text = lines.map((l) => `[${l.id}] ${l.line}`).join("\n");
    const blob = new Blob([text], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `miner-logs-${Date.now()}.txt`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <div style={{ display: "flex", gap: 12, alignItems: "center", fontSize: 12 }}>
        <label style={{ display: "flex", gap: 5, alignItems: "center", cursor: "pointer" }}>
          <input type="checkbox" checked={verbose} onChange={(e) => setVerbose(e.target.checked)} />
          verbose
        </label>
        <label style={{ display: "flex", gap: 5, alignItems: "center", cursor: "pointer" }}>
          <input
            type="checkbox"
            checked={autoscroll}
            onChange={(e) => setAutoscroll(e.target.checked)}
          />
          auto-scroll
        </label>
        <button onClick={exportLogs} style={btn}>
          export
        </button>
        <button onClick={() => setLines([])} style={btn}>
          clear
        </button>
        <span style={{ color: "var(--text-2)", marginLeft: "auto" }}>{shown.length} lines</span>
      </div>
      <div
        className="mono"
        style={{
          height: 360,
          overflowY: "auto",
          background: "var(--bg-0)",
          border: "1px solid var(--border)",
          borderRadius: "var(--radius)",
          padding: 10,
          fontSize: 11.5,
          lineHeight: 1.5,
        }}
      >
        {shown.length === 0 ? (
          <div style={{ color: "var(--text-2)" }}>No log output yet — start a miner.</div>
        ) : (
          shown.map((l, i) => (
            <div key={i} style={{ whiteSpace: "pre-wrap", wordBreak: "break-all" }}>
              <span style={{ color: "var(--text-2)" }}>[{l.id}]</span>{" "}
              <span style={{ color: l.line.includes("[warn]") ? "var(--warn)" : "var(--text-1)" }}>
                {l.line}
              </span>
            </div>
          ))
        )}
        <div ref={endRef} />
      </div>
    </div>
  );
}

const btn: React.CSSProperties = {
  padding: "3px 10px",
  borderRadius: 5,
  border: "1px solid var(--border)",
  background: "var(--bg-2)",
  color: "var(--text-1)",
  cursor: "pointer",
  fontSize: 12,
};
