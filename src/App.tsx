import { useMemo, useState } from "react";
import { Disclaimer } from "./components/Disclaimer";
import { Dashboard } from "./routes/Dashboard";
import { LogViewer } from "./components/LogViewer";
import { Settings } from "./components/Settings";
import { useShortcuts } from "./lib/shortcuts";

type Tab = "dashboard" | "logs" | "settings";

const TABS: { id: Tab; label: string; key: string }[] = [
  { id: "dashboard", label: "Dashboard", key: "1" },
  { id: "logs", label: "Logs", key: "2" },
  { id: "settings", label: "Settings", key: "3" },
];

// App shell: tabbed nav (Dashboard / Logs / Settings) over the lottery
// dashboard, with the permanent "lottery, not income / own hardware only"
// disclaimer kept visible app-wide.
export default function App() {
  const [tab, setTab] = useState<Tab>("dashboard");

  const shortcuts = useMemo(
    () => Object.fromEntries(TABS.map((t) => [t.key, () => setTab(t.id)])),
    [],
  );
  useShortcuts(shortcuts);

  return (
    <div style={{ minHeight: "100%", display: "flex", flexDirection: "column" }}>
      <header
        style={{
          padding: "12px 20px",
          borderBottom: "1px solid var(--border)",
          background: "var(--bg-1)",
          display: "flex",
          alignItems: "center",
          gap: 20,
        }}
      >
        <div>
          <h1 style={{ margin: 0, fontSize: 16, letterSpacing: 0.5 }}>
            <span style={{ color: "var(--accent)" }}>◈</span> Lottery Ticket Terminal
          </h1>
          <p style={{ margin: "2px 0 0", color: "var(--text-2)", fontSize: 12 }}>
            Solo mining orchestrator — your hardware, your odds.
          </p>
        </div>
        <nav style={{ display: "flex", gap: 6, marginLeft: "auto" }}>
          {TABS.map((t) => (
            <button
              key={t.id}
              onClick={() => setTab(t.id)}
              title={`shortcut: ${t.key}`}
              style={{
                padding: "6px 14px",
                borderRadius: 6,
                border: "1px solid var(--border)",
                background: tab === t.id ? "var(--bg-3)" : "transparent",
                color: tab === t.id ? "var(--accent)" : "var(--text-1)",
                cursor: "pointer",
                fontSize: 13,
              }}
            >
              {t.label}
            </button>
          ))}
        </nav>
      </header>

      <main style={{ flex: 1, padding: 20, overflowY: "auto" }}>
        {tab === "dashboard" && <Dashboard />}
        {tab === "logs" && <LogViewer />}
        {tab === "settings" && <Settings />}
      </main>

      <Disclaimer />
    </div>
  );
}
