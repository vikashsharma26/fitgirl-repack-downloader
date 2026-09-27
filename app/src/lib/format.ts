const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function bytes(n: number | null | undefined): string {
  if (n == null) return "?";
  let v = n;
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 || i === 0 ? v.toFixed(0) : v.toFixed(1)} ${UNITS[i]}`;
}

export function speed(n: number): string {
  return n > 0 ? `${bytes(n)}/s` : "";
}

export function eta(secs: number | null | undefined): string {
  if (secs == null || !isFinite(secs)) return "";
  if (secs < 60) return `${Math.max(1, Math.round(secs))}s`;
  const m = Math.floor(secs / 60);
  if (m < 60) return `${m}m ${Math.round(secs % 60)}s`;
  const h = Math.floor(m / 60);
  return `${h}h ${m % 60}m`;
}

export function percent(done: number, total: number | null | undefined): number {
  if (!total) return 0;
  return Math.min(100, (done / total) * 100);
}
