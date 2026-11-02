/**
 * The colour theme of the web app. The standard asks for both themes, for following the system, and for an explicit
 * override when that is useful (`docs/design-web.md`), so the app keeps one preference — `system`,
 * `light` or `dark` — in local storage, and
 * writes the resolved theme on `<html data-theme>` where `src/styles/theme.css` reads it.
 *
 * The script in `index.html` reads the same key before the app loads, so the first picture is already in the right
 * theme; `start` then keeps it in step with the system, and `useTheme` is how a page offers the override.
 *
 * Storage is a courtesy, not a guarantee: a browser that blocks it throws on every access, so nothing here lets a
 * refused read stop the app from drawing, or a refused write stop a choice from taking effect. The theme is applied
 * first and remembered second.
 */
import { useCallback, useSyncExternalStore } from "react";

/** What a person picked; `system` follows the operating system. */
export type ThemePreference = "system" | "light" | "dark";
/** The theme the app is drawn in. */
export type Theme = "light" | "dark";
/** The preference and the theme it gives, as the one value a page reads. */
export interface ThemeState {
  preference: ThemePreference;
  theme: Theme;
}

const key = "moebius.theme";

function preference(): ThemePreference {
  try {
    const stored = localStorage.getItem(key);
    return stored === "light" || stored === "dark" ? stored : "system";
  } catch {
    // A blocked storage is a person who has not chosen, not a page that cannot start.
    return "system";
  }
}

/** The question the operating system answers, watched for as long as the page lasts. */
function system(): MediaQueryList | null {
  if (typeof matchMedia === "undefined") return null;
  if (!following) {
    following = matchMedia("(prefers-color-scheme: light)");
    following.addEventListener("change", notify);
  }
  return following;
}
let following: MediaQueryList | null = null;

/** The theme a preference resolves to right now. */
function resolve(preference: ThemePreference): Theme {
  if (preference !== "system") return preference;
  return system()?.matches ? "light" : "dark";
}

/** Applies a preference to the document and remembers it; a browser that refuses to remember is not obeyed. */
function apply(next: ThemePreference): Theme {
  const theme = resolve(next);
  if (typeof document !== "undefined") document.documentElement.dataset.theme = theme;
  return theme;
}

/**
 * Called by `main.tsx` before the app draws: sets the theme and starts watching the system, whether or not the
 * preference is `system` now, so choosing to follow it later works without a reload.
 */
export function start(): ThemePreference {
  const saved = preference();
  apply(saved);
  system();
  return saved;
}

const listeners = new Set<() => void>();

// The value pages read. It changes when either half does, so a system flip under a `system` preference is a change a
// page is told about; returning the preference alone would hand back the same string and nothing would redraw.
let current: ThemeState | null = null;

function state(): ThemeState {
  const saved = preference();
  const theme = resolve(saved);
  if (!current || current.preference !== saved || current.theme !== theme) {
    current = { preference: saved, theme };
  }
  return current;
}

function notify(): void {
  apply(preference());
  state();
  for (const listener of listeners) listener();
}

function setPreference(next: ThemePreference): void {
  apply(next);
  try {
    localStorage.setItem(key, next);
  } catch {
    // The theme is right in front of the person and simply not remembered for the next visit.
  }
  state();
  for (const listener of listeners) listener();
}

/** The preference and the theme it gives, and the way to change it. */
export function useTheme(): ThemeState & { setPreference: (next: ThemePreference) => void } {
  const saved = useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      // Following the system is watched from the moment a page asks what the theme is, not only when the page
      // opened to a `system` preference.
      system();
      return () => listeners.delete(listener);
    },
    state,
    state,
  );
  const change = useCallback((next: ThemePreference) => setPreference(next), []);
  return { ...saved, setPreference: change };
}
