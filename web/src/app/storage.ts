/** A per-viewer convenience kept in localStorage; storage can be blocked or empty, so every access is guarded. */
export function readFlag(key: string): boolean {
  try {
    return window.localStorage.getItem(key) === "1";
  } catch {
    return false;
  }
}

export function writeFlag(key: string, value: boolean): void {
  try {
    window.localStorage.setItem(key, value ? "1" : "0");
  } catch {
    // The setting is only not remembered.
  }
}

/** A word or number kept for this browser alone; null when nothing was kept or storage refuses. */
export function readText(key: string): string | null {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function writeText(key: string, value: string): void {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // The setting is only not remembered.
  }
}
