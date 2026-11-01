/**
 * The words and numbers of the Decision model page, as `screens/decision_model.rs` of the desktop app has them: a share
 * as a percentage, a time as a named day and a UTC clock, the route that downloads a kind's file, and how the shadowing of a kind is told.
 */
import type { Export, KindCounts, ShadowedKind } from "../../api/types/improve";
import { dayName, today } from "../../ui/lib/format";

/** A share between 0 and 1 as a percentage, such as `97%` or `96.5%`. */
export function percent(share: number): string {
  const rounded = Math.round(share * 1000) / 10;
  return Number.isInteger(rounded) ? `${rounded.toFixed(0)}%` : `${rounded.toFixed(1)}%`;
}

/**
 * `at` (Unix seconds) as a day in words and a time in UTC, such as `Today 10:15 UTC` or `26 Oct 07:33 UTC`. The
 * clock stays UTC because the server wrote the file by it; only the day is named, as every other day in the app is.
 */
export function when(at: number, now = today()): string {
  const iso = new Date(at * 1000).toISOString();
  return `${dayName(iso.slice(0, 10), now)} ${iso.slice(11, 16)} UTC`;
}

/** The route that downloads the file of `kind` in the newest export that has one. */
export function exportRoute(exports: Export[], kind: string): string | null {
  const found = exports.find((each) => each.kinds.some((k) => k.kind === kind));
  return found ? `GET /v1/exports/${found.id}/${kind}.jsonl` : null;
}

/** How many judgements the server holds in all kinds. */
export function judgements(kinds: KindCounts[]): number {
  return kinds.reduce((sum, kind) => sum + kind.count, 0);
}

/** The shadows of a kind with how many failed and were dropped: `40, 3 failed, 2 dropped`. */
export function shadowsLine(kind: ShadowedKind): string {
  const parts = [String(kind.shadows)];
  if (kind.failed > 0) parts.push(`${kind.failed} failed`);
  if (kind.dropped > 0) parts.push(`${kind.dropped} dropped`);
  return parts.join(", ");
}

/** The agreement of a kind against the share a takeover needs: `85% (needs 95%)`. */
export function agreementLine(share: number, needed: number): string {
  return `${percent(share)} (needs ${percent(needed)})`;
}

/** `localize` is measured by recall, not agreement: the model's recall against the recorded hints'. */
export function recallLine(kind: ShadowedKind): string | null {
  if (kind.recall == null) return null;
  const base =
    kind.baseline_recall == null ? "" : ` against ${percent(kind.baseline_recall)} of the hints`;
  return `Recall ${percent(kind.recall)}${base}`;
}
