// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { start, useBlur } from "./effects";

afterEach(() => {
  cleanup();
  localStorage.clear();
});

/** jsdom has no WebGL, so `start` finds no GPU; a GPU is simulated by a canvas that names one. */
function withRenderer(name: string | null) {
  const doc = document.implementation.createHTMLDocument("");
  const real = doc.createElement.bind(doc);
  doc.createElement = ((tag: string) => {
    if (tag !== "canvas") return real(tag);
    return {
      getContext: () =>
        name === null
          ? null
          : {
              getExtension: () => ({ UNMASKED_RENDERER_WEBGL: 1, loseContext: () => {} }),
              getParameter: () => name,
              RENDERER: 0,
            },
    } as unknown as HTMLCanvasElement;
  }) as typeof doc.createElement;
  return doc;
}

describe("the Blur on menus switch", () => {
  it("is off and cannot be turned on where the renderer is software", () => {
    const doc = withRenderer("llvmpipe (LLVM 15)");
    start(doc);
    const { result } = renderHook(() => useBlur());
    expect(result.current.gpu).toBe(false);
    expect(result.current.on).toBe(false);
    expect(doc.documentElement.dataset.blur).toBe("off");
  });

  it("is on with a GPU, and turning it off writes data-blur off and is kept", () => {
    start(withRenderer("Apple M2"));
    const { result } = renderHook(() => useBlur());
    expect(result.current.gpu).toBe(true);
    expect(result.current.on).toBe(true);
    act(() => result.current.setOn(false));
    expect(result.current.on).toBe(false);
    expect(document.documentElement.dataset.blur).toBe("off");
    expect(localStorage.getItem("moebius.blur")).toBe("off");
    act(() => result.current.setOn(true));
    expect(document.documentElement.dataset.blur).toBe("on");
  });
});
