/**
 * The fuzzy match of "Go to file": the characters typed must occur in a path in that order, ignoring case and blanks.
 * Characters that follow each other score more, and so do those that start a word (after `/`, `_`, `-`, `.` or a
 * space, or a capital after a lowercase letter) or fall in the file's name rather than its directories; a gap costs a
 * point per character skipped, a file named as typed wins, and a shorter path wins a tie.
 */

const MATCH = 1;
const CONSECUTIVE = 12;
const WORD_START = 16;
const IN_NAME = 6;
const EXACT_NAME = 30;
const GAP = 1;

const NONE = Number.NEGATIVE_INFINITY;

/** How well `query` matches `path`: null when its characters do not occur in order. Higher is better. */
export function score(query: string, path: string): number | null {
  const wanted = [...query.toLowerCase()].filter((char) => char.trim() !== "");
  if (wanted.length === 0) return 0;
  const chars = [...path];
  const lower = chars.map((char) => char.toLowerCase());
  const name = chars.lastIndexOf("/") + 1;
  const worth = (at: number) => {
    const before = chars[at - 1] ?? "";
    const here = chars[at] ?? "";
    const boundary =
      at === 0 ||
      "/_-. ".includes(before) ||
      (before !== before.toUpperCase() && here !== here.toLowerCase());
    return MATCH + (boundary ? WORD_START : 0) + (at >= name ? IN_NAME : 0);
  };
  // best[j] is the best score of the characters matched so far with the last one at j.
  let best: number[] = chars.map(() => NONE);
  wanted.forEach((want, index) => {
    const now: number[] = chars.map(() => NONE);
    // The best `best[k] + k` over the k that leave a gap before j (k <= j - 2).
    let apart = NONE;
    for (let at = 0; at < chars.length; at += 1) {
      const far = best[at - 2];
      if (at >= 2 && far !== undefined && far > NONE) apart = Math.max(apart, far + (at - 2));
      if (lower[at] !== want) continue;
      if (index === 0) {
        now[at] = worth(at) - at * GAP;
        continue;
      }
      let top = NONE;
      const adjacent = best[at - 1];
      if (at >= 1 && adjacent !== undefined && adjacent > NONE) {
        top = adjacent + CONSECUTIVE;
      }
      if (apart > NONE) top = Math.max(top, apart - (at - 1) * GAP);
      if (top > NONE) now[at] = top + worth(at);
    }
    best = now;
  });
  const top = Math.max(...best);
  if (top === NONE) return null;
  const file = path.slice(name).toLowerCase();
  const typed = wanted.join("");
  const stem = file.includes(".") ? file.slice(0, file.lastIndexOf(".")) : file;
  const exact = file === typed || stem === typed ? EXACT_NAME : 0;
  return top + exact - path.length / 100;
}

/** The paths that match `query`, the best first, at most `limit`; every path in order for a blank query. */
export function rank(query: string, paths: readonly string[], limit: number): string[] {
  const scored: Array<{ path: string; score: number }> = [];
  for (const path of paths) {
    const each = score(query, path);
    if (each !== null) scored.push({ path, score: each });
  }
  return scored
    .sort((a, b) => b.score - a.score || a.path.localeCompare(b.path))
    .slice(0, limit)
    .map((each) => each.path);
}
