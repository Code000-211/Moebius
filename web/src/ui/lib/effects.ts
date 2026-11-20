/**
 * Whether the machine can blur what is behind a small menu without stutter. Blur is a GPU job: on a software renderer
 * (llvmpipe, SwiftShader, softpipe, a basic render driver) it costs every frame, so the `glass` utility then stays the
 * opaque panel. `start` decides once at launch from the graphics renderer's name and writes the answer on the root as
 * `data-blur="on"` or `"off"`, where `tailwind.css` reads it.
 */

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

/** Sets `data-blur` on the root from the renderer. */
export function start(doc: Document = document): void {
  doc.documentElement.dataset.blur = blurAllowed(rendererName(doc)) ? "on" : "off";
}
