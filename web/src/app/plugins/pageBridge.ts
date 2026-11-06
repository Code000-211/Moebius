/**
 * What the app accepts from the sandboxed frame a plugin's page runs in. The frame has an opaque origin and the plugin's
 * code in it is not trusted, so nothing it says is believed beyond this: a message is a plain object of one of five
 * types, the values it reports are strings for settings the plugin declares, and a height is a number the app clamps.
 * Anything else is dropped. Who sent a message is checked by the caller (`event.source` is the frame's window).
 */
import type { PluginSetting } from "../../api/types/projects";

/** The most a reported value may be, as long as the server takes (`MAX_VALUE` in `app/plugin_settings.rs`). */
export const MAX_VALUE_BYTES = 256 * 1024;
/** The tallest a plugin's page may make its frame, in pixels. */
export const MAX_HEIGHT = 20_000;

export type FrameMessage =
  | { type: "ready" }
  | { type: "mounted" }
  | { type: "failed"; message: string }
  | { type: "height"; height: number }
  | { type: "change"; values: Record<string, string> };

function isPlain(data: unknown): data is Record<string, unknown> {
  return typeof data === "object" && data !== null && !Array.isArray(data);
}

/** The values a `change` message carries, or null when any key is not declared, any value is not text, a one-line
 * setting has a line break, or a value is too long: the app checks them as it checks the fields. */
function checkedValues(raw: unknown, declared: PluginSetting[]): Record<string, string> | null {
  if (!isPlain(raw)) return null;
  const values: Record<string, string> = {};
  for (const [key, value] of Object.entries(raw)) {
    const setting = declared.find((each) => each.key === key);
    if (!setting || typeof value !== "string") return null;
    if (setting.input === "line" && /[\r\n]/.test(value)) return null;
    if (new TextEncoder().encode(value).length > MAX_VALUE_BYTES) return null;
    values[key] = value;
  }
  return values;
}

/** The message in `data` when it is one the frame may send, else null. */
export function readFrameMessage(data: unknown, declared: PluginSetting[]): FrameMessage | null {
  if (!isPlain(data)) return null;
  switch (data.type) {
    case "ready":
    case "mounted":
      return { type: data.type };
    case "failed":
      return {
        type: "failed",
        message: typeof data.message === "string" ? data.message.slice(0, 500) : "it failed",
      };
    case "height": {
      const height = data.height;
      if (typeof height !== "number" || !Number.isFinite(height) || height < 0) return null;
      return { type: "height", height: Math.min(Math.ceil(height), MAX_HEIGHT) };
    }
    case "change": {
      const values = checkedValues(data.values, declared);
      return values ? { type: "change", values } : null;
    }
    default:
      return null;
  }
}

/** The colour theme the app shows now, as the frame is told it. */
export function currentTheme(): "light" | "dark" {
  return document.documentElement.dataset.theme === "light" ? "light" : "dark";
}

/** The app's stylesheets as one text, so the frame looks like the app: the built files and any injected rules. */
export async function appStyles(): Promise<string> {
  const parts = await Promise.all(
    Array.from(document.querySelectorAll<HTMLElement>('link[rel="stylesheet"], style')).map(
      async (element) => {
        if (element instanceof HTMLLinkElement) {
          try {
            const response = await fetch(element.href, { credentials: "same-origin" });
            return response.ok ? await response.text() : "";
          } catch {
            return "";
          }
        }
        return element.textContent ?? "";
      },
    ),
  );
  return parts.join("\n");
}
