// Permanent, unavoidable disclaimer. Solo mining is a high-variance lottery,
// not income, and this app is for mining the user's OWN hardware with consent.
// This component must remain mounted app-wide (spec requirement).
export function Disclaimer() {
  return (
    <footer
      style={{
        padding: "8px 20px",
        borderTop: "1px solid var(--border)",
        background: "var(--bg-1)",
        color: "var(--text-2)",
        fontSize: 11,
        lineHeight: 1.4,
      }}
    >
      <strong style={{ color: "var(--warn)" }}>⚠ High-variance lottery, not income.</strong>{" "}
      Solo mining typically yields nothing for years; payouts are rare and random. Use only on
      hardware you own and consent to run. This tool never handles private keys — addresses only.
    </footer>
  );
}
