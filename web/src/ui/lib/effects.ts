/**
 * Whether the machine can blur what is behind a small menu without stutter. Blur is a GPU job: on a software renderer
 * (llvmpipe, SwiftShader, softpipe, a basic render driver) it costs every frame, so the `glass` utility then stays the
 * opaque panel. `start` decides once at launch from the graphics renderer's name and writes the answer on the root as
 * `data-blur="on"` or `"off"`, where `tailwind.css` reads it.
 */
import { useCallback, useSyncExternalStore } from "react";

const SOFTWARE =
  /llvmpipe|swiftshader|softpipe|software|basic render|microsoft basic|mesa offscreen|lavapipe/i;

/** Whether the renderer named is a GPU. An unknown name (none could be read) counts as software: blur is the extra. */
export function blurAllowed(renderer: string | null | undefined): boolean {
  if (!renderer || renderer.trim() === "") return false;
  return !SOFTWARE.test(renderer);
}

/** The name of the graphics renderer, as the browser reports it, or null where it will not say. */
export function rendererName(doc: Document = document): string | null {
  try {
    const gl = doc.createElement("canvas").getContext("webgl");
    if (!gl) return null;
    const info = gl.getExtension("WEBGL_debug_renderer_info");
    const name = gl.getParameter(info ? info.UNMASKED_RENDERER_WEBGL : gl.RENDERER);
    gl.getExtension("WEBGL_lose_context")?.loseContext();
    return typeof name === "string" ? name : null;
  } catch {
    return null;
  }
}

const KEY = "moebius.blur";

/** Whether the machine can blur, from the renderer; `start` reads it once. */
let gpu = false;
/** What the person chose in this page, kept in memory in case storage refuses it. */
let chosen: boolean | null = null;

function savedChoice(): boolean {
  if (chosen !== null) return chosen;
  try {
    return localStorage.getItem(KEY) !== "off";
  } catch {
    return true;
  }
}

function apply(doc: Document): void {
  doc.documentElement.dataset.blur = gpu && savedChoice() ? "on" : "off";
}

/** Sets `data-blur` on the root from the renderer and from the person's choice (on unless switched off). */
export function start(doc: Document = document): void {
  gpu = blurAllowed(rendererName(doc));
  apply(doc);
}

const listeners = new Set<() => void>();

/** Blur on menus as the Appearance switch shows it: on only with a GPU and while the person has not switched it off. */
export function useBlur(): { on: boolean; gpu: boolean; setOn: (on: boolean) => void } {
  const on = useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => gpu && savedChoice(),
    () => false,
  );
  const setOn = useCallback((next: boolean) => {
    chosen = next;
    try {
      localStorage.setItem(KEY, next ? "on" : "off");
    } catch {
      // The switch is in effect and simply not remembered for the next visit.
    }
    apply(document);
    for (const listener of listeners) listener();
  }, []);
  return { on, gpu, setOn };
}
