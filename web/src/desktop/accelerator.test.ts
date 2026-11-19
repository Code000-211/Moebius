import { describe, expect, it } from "vitest";
import { acceleratorOf } from "./accelerator";

const press = (
  code: string,
  held: Partial<Record<"ctrl" | "alt" | "shift" | "meta", true>> = {},
  key = code,
) => ({
  code,
  key,
  ctrlKey: !!held.ctrl,
  altKey: !!held.alt,
  shiftKey: !!held.shift,
  metaKey: !!held.meta,
});

describe("acceleratorOf", () => {
  it("writes the modifiers in the order the app writes them, then the key", () => {
    expect(acceleratorOf(press("Space", { shift: true, ctrl: true }, " "))).toBe(
      "Ctrl+Shift+Space",
    );
    expect(acceleratorOf(press("KeyK", { meta: true, alt: true }, "k"))).toBe("Alt+Super+K");
    expect(acceleratorOf(press("Digit7", { ctrl: true }, "7"))).toBe("Ctrl+7");
    expect(acceleratorOf(press("F12", { ctrl: true }, "F12"))).toBe("Ctrl+F12");
    expect(acceleratorOf(press("ArrowUp", { alt: true }, "ArrowUp"))).toBe("Alt+ArrowUp");
  });

  it("waits while only modifiers are held and refuses keys a shortcut cannot use", () => {
    expect(acceleratorOf(press("ControlLeft", { ctrl: true }, "Control"))).toBeNull();
    expect(acceleratorOf(press("ShiftRight", { shift: true }, "Shift"))).toBeNull();
    expect(acceleratorOf(press("Semicolon", { ctrl: true }, ";"))).toBeNull();
  });
});
