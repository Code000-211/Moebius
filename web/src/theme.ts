/**
 * The colour theme of the web app. The standard asks for both themes, for following the system, and for an explicit
 * override when that is useful (`docs/design-web.md`), so the app keeps one preference — `system`,
 * `light` or `dark` — in local storage, and
 * writes the resolved theme on `<html data-theme>` where `src/styles/theme.css` reads it.
 *
 * The script in `index.html` reads the same key before the app loads, so the first picture is already in the right
 * theme; `start` then keeps it in step with the system, and `useTheme` is how a page offers the override.
 */
import { useCallback, useSyncExternalStore } from "react";

/** What a person picked; `system` follows the operating system. */
export type ThemePreference = "system" | "light" | "dark";
/** The theme the app is drawn in. */
export type Theme = "light" | "dark";

const key = "moebius.theme";

function preference(): ThemePreference {
  const stored = typeof localStorage === "undefined" ? null : localStorage.getItem(key);
  return stored === "light" || stored === "dark" ? stored : "system";
}

/** The theme a preference resolves to right now. */
export function resolve(preference: ThemePreference): Theme {
  if (preference !== "system") return preference;
  return typeof matchMedia !== "undefined" && matchMedia("(prefers-color-scheme: light)").matches
    ? "light"
    : "dark";
}

/** Applies a preference to the document and remembers it. */
export function apply(next: ThemePreference): Theme {
  if (typeof document === "undefined") return resolve(next);
  document.documentElement.dataset.theme = resolve(next);
  return resolve(next);
}

/**
 * Called by `index.html` before the app runs: sets the theme and keeps it in step with the system while the
 * preference is `system`.
 */
export function start(): ThemePreference {
  const saved = preference();
  apply(saved);
  if (saved === "system" && typeof matchMedia !== "undefined") {
    matchMedia("(prefers-color-scheme: light)").addEventListener("change", () =>
      apply(preference()),
    );
  }
  return saved;
}

const listeners = new Set<() => void>();

function setPreference(next: ThemePreference): void {
  localStorage.setItem(key, next);
  apply(next);
  for (const listener of listeners) listener();
}

/** The preference and the theme it gives, and the way to change it. */
export function useTheme(): {
  preference: ThemePreference;
  theme: Theme;
  setPreference: (next: ThemePreference) => void;
} {
  const saved = useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    preference,
    () => "system" as ThemePreference,
  );
  const change = useCallback((next: ThemePreference) => setPreference(next), []);
  return { preference: saved, theme: resolve(saved), setPreference: change };
}
