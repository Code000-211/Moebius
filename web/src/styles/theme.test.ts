// @vitest-environment node
/**
 * Keeps theme.css in step with the desktop app's `theme.rs`: every colour constant of theme.rs is a variable of the
 * same name in kebab case with the same value, and the radii and spacing scale match.
 */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const rust = readFileSync(
  new URL("../../../crates/moebius-desktop/src/theme.rs", import.meta.url),
  "utf8",
);
const css = readFileSync(new URL("./theme.css", import.meta.url), "utf8");

const kebab = (name: string) => name.toLowerCase().replaceAll("_", "-");

function variables(): Map<string, string> {
  const found = new Map<string, string>();
  for (const [, name, value] of css.matchAll(/--([a-z0-9-]+):\s*([^;]+);/g)) {
    found.set(name as string, (value as string).trim().toLowerCase());
  }
  return found;
}

describe("theme.css", () => {
  const vars = variables();

  it("has every colour of theme.rs with its value", () => {
    const colours = [
      ...rust.matchAll(/pub const ([A-Z_0-9]+): Color = hex\(0x([0-9A-Fa-f]{6})\);/g),
    ];
    expect(colours.length).toBeGreaterThan(40);
    for (const [, name, hex] of colours) {
      expect(vars.get(kebab(name as string)), `--${kebab(name as string)}`).toBe(
        `#${(hex as string).toLowerCase()}`,
      );
    }
  });

  it("has the colours of the models of the Usage page", () => {
    const list = /const MODEL_COLOURS: \[Color; \d+\] = \[([^\]]+)\]/.exec(rust)?.[1] ?? "";
    const hexes = [...list.matchAll(/hex\(0x([0-9A-Fa-f]{6})\)/g)].map((m) =>
      (m[1] as string).toLowerCase(),
    );
    expect(hexes).toHaveLength(6);
    hexes.forEach((hex, at) => {
      expect(vars.get(`model-${at + 1}`), `--model-${at + 1}`).toBe(`#${hex}`);
    });
  });

  it("has the radii of theme.rs", () => {
    const radii = [...rust.matchAll(/pub const RADIUS_([A-Z]+): f32 = ([0-9.]+);/g)];
    expect(radii.length).toBeGreaterThan(5);
    for (const [, name, value] of radii) {
      expect(vars.get(`radius-${(name as string).toLowerCase()}`), `RADIUS_${name}`).toBe(
        `${Number(value)}px`,
      );
    }
  });

  it("has the spacing scale of theme.rs", () => {
    const spaces = [...rust.matchAll(/pub const SPACE_([0-9]+): f32 = ([0-9.]+);/g)];
    expect(spaces.length).toBe(12);
    for (const [, name, value] of spaces) {
      expect(vars.get(`space-${name}`), `SPACE_${name}`).toBe(`${Number(value)}px`);
    }
  });

  it("has the widths of the sidebar and the connect card", () => {
    const width = (name: string) =>
      Number(new RegExp(`pub const ${name}: f32 = ([0-9.]+);`).exec(rust)?.[1]);
    expect(vars.get("sidebar-width")).toBe(`${width("SIDEBAR_WIDTH")}px`);
    expect(vars.get("rail-width")).toBe(`${width("RAIL_WIDTH")}px`);
    expect(vars.get("connect-width")).toBe(`${width("CONNECT_WIDTH")}px`);
  });
});
