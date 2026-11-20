// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { start } from "../../ui/lib/effects";
import { Appearance } from "./Appearance";

afterEach(() => {
  cleanup();
  localStorage.clear();
});

describe("Appearance", () => {
  it("lays Theme, Reduce motion and Blur on menus out as rows with switches", () => {
    render(<Appearance />);
    expect(screen.getByRole("radiogroup", { name: "Theme" })).toBeTruthy();
    const motion = screen.getByRole("switch", { name: "Reduce motion" });
    expect(motion.getAttribute("aria-checked")).toBe("false");
    fireEvent.click(motion);
    expect(motion.getAttribute("aria-checked")).toBe("true");
    expect(document.documentElement.dataset.reduceMotion).toBe("true");
    fireEvent.click(motion);
  });

  it("says blur is off automatically without a GPU and leaves the switch off", () => {
    start(); // jsdom has no WebGL: no GPU
    render(<Appearance />);
    const blur = screen.getByRole("switch", { name: "Blur on menus" }) as HTMLButtonElement;
    expect(blur.disabled).toBe(true);
    expect(blur.getAttribute("aria-checked")).toBe("false");
    expect(screen.getByText("Off automatically without GPU")).toBeTruthy();
  });
});
