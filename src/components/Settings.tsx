import { useEffect, useState } from "react";
import {
  getSettings,
  updateSettings,
  exportConfig,
  importConfig,
  checkUpdates,
  type GlobalSettings,
  type UpdateInfo,
} from "../lib/ipc";

const card: React.CSSProperties = {
  background: "var(--bg-1)",
  border: "1px solid var(--border)",
  borderRadius: "var(--radius)",
  padding: 16,
  display: "flex",
  flexDirection: "column",
  gap: 12,
};
const btn: React.CSSProperties = {
  padding: "5px 12px",
  borderRadius: 6,
  border: "1px solid var(--border)",
  background: "var(--bg-2)",
  color: "var(--text-1)",
  cursor: "pointer",
  fontSize: 13,
};

export function Settings() {
  const [settings, setSettings] = useState<GlobalSettings | null>(null);
  const [updates, setUpdates] = useState<UpdateInfo[] | null>(null);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    getSettings().then(setSettings).catch(() => {});
  }, []);

  const patch = (p: Partial<GlobalSettings>) =>
    setSettings((s) => (s ? { ...s, ...p } : s));

  const save = async () => {
    if (!settings) return;
    await updateSettings(settings);
    setSaved(true);
    setTimeout(() => setSaved(false), 1500);
  };

  const doExport = async () => {
    const json = await exportConfig();
    const blob = new Blob([json], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `pcmined-config-${Date.now()}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const doImport = (file: File) => {
    const reader = new FileReader();
    reader.onload = async () => {
      await importConfig(String(reader.result));
      getSettings().then(setSettings).catch(() => {});
    };
    reader.readAsText(file);
  };

  if (!settings) return <div style={{ color: "var(--text-2)" }}>Loading settings…</div>;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16, maxWidth: 640 }}>
      <section style={card}>
        <h3 style={{ margin: 0, fontSize: 14 }}>Preferences</h3>

        <Row label="Theme">
          <select
            value={settings.theme}
            onChange={(e) => patch({ theme: e.target.value })}
            style={btn}
          >
            <option value="dark">dark</option>
            <option value="midnight">midnight</option>
          </select>
        </Row>

        <Row label="Minimize to tray">
          <input
            type="checkbox"
            checked={settings.minimize_to_tray}
            onChange={(e) => patch({ minimize_to_tray: e.target.checked })}
          />
        </Row>

        <Row label="Log level">
          <select
            value={settings.log_level}
            onChange={(e) => patch({ log_level: e.target.value })}
            style={btn}
          >
            {["error", "warn", "info", "debug", "trace"].map((l) => (
              <option key={l} value={l}>
                {l}
              </option>
            ))}
          </select>
        </Row>

        <Row label="Binaries dir">
          <input
            value={settings.binaries_dir ?? ""}
            placeholder="<app-data>/bin"
            onChange={(e) => patch({ binaries_dir: e.target.value || null })}
            style={{ ...btn, minWidth: 280, cursor: "text" }}
          />
        </Row>

        <div style={{ display: "flex", gap: 10, alignItems: "center" }}>
          <button onClick={save} style={{ ...btn, color: "var(--accent)" }}>
            Save
          </button>
          {saved && <span style={{ color: "var(--accent)", fontSize: 12 }}>saved ✓</span>}
        </div>
      </section>

      <section style={card}>
        <h3 style={{ margin: 0, fontSize: 14 }}>Miner binaries</h3>
        <button onClick={() => checkUpdates().then(setUpdates)} style={btn}>
          Check for updates
        </button>
        {updates && (
          <div className="mono" style={{ fontSize: 12 }}>
            {updates.map((u) => (
              <div key={u.miner_id} style={{ display: "flex", justifyContent: "space-between" }}>
                <span>{u.miner_id}</span>
                <span style={{ color: u.update_available ? "var(--warn)" : "var(--text-2)" }}>
                  {u.installed ?? "not installed"} → {u.available}
                  {u.update_available ? " (update)" : ""}
                </span>
              </div>
            ))}
          </div>
        )}
      </section>

      <section style={card}>
        <h3 style={{ margin: 0, fontSize: 14 }}>Config backup</h3>
        <div style={{ display: "flex", gap: 10, alignItems: "center" }}>
          <button onClick={doExport} style={btn}>
            Export backup
          </button>
          <label style={{ ...btn, display: "inline-block" }}>
            Restore…
            <input
              type="file"
              accept="application/json"
              style={{ display: "none" }}
              onChange={(e) => e.target.files?.[0] && doImport(e.target.files[0])}
            />
          </label>
        </div>
      </section>
    </div>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
      <span style={{ color: "var(--text-1)", fontSize: 13 }}>{label}</span>
      {children}
    </div>
  );
}
