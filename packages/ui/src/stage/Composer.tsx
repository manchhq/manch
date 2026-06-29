export function Composer({
  value, onChange, onSend, busy = false,
}: {
  value: string;
  onChange: (next: string) => void;
  onSend: () => void;
  busy?: boolean;
}) {
  const canSend = !busy && value.trim().length > 0;
  return (
    <form
      className="flex items-center gap-2 border-t border-base-300 p-3"
      onSubmit={(e) => { e.preventDefault(); if (canSend) onSend(); }}
    >
      <input
        className="input input-bordered flex-1"
        placeholder="Message…"
        value={value}
        disabled={busy}
        onChange={(e) => onChange(e.target.value)}
      />
      <button type="submit" className="btn btn-primary" aria-label="Send" disabled={!canSend}>
        {busy ? <span className="loading loading-dots loading-sm" /> : "Send"}
      </button>
    </form>
  );
}
