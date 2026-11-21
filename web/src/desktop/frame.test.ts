/** The page is marked as the desktop app's only when the app's bridge is there. */
import { afterEach, describe, expect, it } from "vitest";
import { fakeBridge } from "./fakeBridge";
import { start } from "./frame";

afterEach(() => {
  delete window.moebiusDesktop;
});

describe("frame", () => {
  it("marks nothing in a browser", () => {
    const root = document.createElement("html");
    start(root);
    expect(root.hasAttribute("data-desktop")).toBe(false);
  });

  it("marks the page of the desktop app", () => {
    window.moebiusDesktop = fakeBridge();
    const root = document.createElement("html");
    start(root);
    expect(root.hasAttribute("data-desktop")).toBe(true);
  });
});
