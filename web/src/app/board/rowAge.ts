/**
 * The age a list row shows, in the compact way of the rows: `5m`, `3h`, `2d`; "now" for less than a minute.
 */
export function rowAge(createdAt: number, now = Date.now() / 1000): string {
  const seconds = Math.max(0, Math.floor(now - createdAt));
  if (seconds < 60) return "now";
  if (seconds < 3_600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3_600)}h`;
  return `${Math.floor(seconds / 86_400)}d`;
}
