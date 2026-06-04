import { Disclaimer } from "./components/Disclaimer";
import { Dashboard } from "./routes/Dashboard";

// App shell: header, the lottery dashboard, and the permanent "lottery, not
// income / own hardware only" disclaimer that must remain visible app-wide.
export default function App() {
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
        <Dashboard />
      </main>

      <Disclaimer />
    </div>
  );
}
