import { useEffect, useState } from "react";
import { ping } from "./lib/ipc";
import { Disclaimer } from "./components/Disclaimer";

// P0 shell: proves the IPC boundary round-trips and renders the permanent
// "lottery, not income" disclaimer that must remain visible app-wide.
export default function App() {
  const [pong, setPong] = useState<string>("…");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    ping("hello from webview")
      .then(setPong)
      .catch((e) => setError(String(e)));
  }, []);

  return (
    <div style={{ minHeight: "100%", display: "flex", flexDirection: "column" }}>
      <header
        style={{
          padding: "14px 20px",
          borderBottom: "1px solid var(--border)",
          background: "var(--bg-1)",
        }}
      >
        <h1 style={{ margin: 0, fontSize: 16, letterSpacing: 0.5 }}>
          <span style={{ color: "var(--accent)" }}>◈</span> Lottery Ticket Terminal
        </h1>
        <p style={{ margin: "2px 0 0", color: "var(--text-2)", fontSize: 12 }}>
          Solo mining orchestrator — your hardware, your odds.
        </p>
      </header>

      <main style={{ flex: 1, padding: 20 }}>
        <section
          className="mono"
          style={{
            background: "var(--bg-1)",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            padding: 16,
            maxWidth: 560,
          }}
        >
          <div style={{ color: "var(--text-2)", fontSize: 12, marginBottom: 6 }}>
            IPC boundary check (ping → Rust core → pong)
          </div>
          {error ? (
            <div style={{ color: "var(--danger)" }}>error: {error}</div>
          ) : (
            <div style={{ color: "var(--accent)" }}>{pong}</div>
          )}
        </section>
      </main>

      <Disclaimer />
    </div>
  );
}
