// @vitest-environment node
/**
 * Keeps the tokens of `theme.css` and `tailwind.css` inside the design standard,
 * [`docs/design-web.md`](../../../docs/design-web.md). The web app follows that standard rather than the desktop app's
 * Deep Velvet palette ([ADR 0016](../../../docs/adr/0016-the-web-app-has-its-own-design-standard.md)), so this test reads the standard's own numbers
 * rather than `theme.rs`: both themes define the same names, every radius and every step of the type and spacing scale
 * is one the standard lists, text is readable on the surface it sits on in both themes, and no colour literal appears
 * anywhere else in the app. It also fails a name that no theme defines, because a declaration that names a token which
 * is not there is dropped in silence and the control simply loses its colour, its border or its focus ring.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { basename, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("./theme.css", import.meta.url), "utf8");
const tailwind = readFileSync(new URL("./tailwind.css", import.meta.url), "utf8");
const srcRoot = fileURLToPath(new URL("..", import.meta.url));

/** The radii the standard allows: cards and sheets, controls, small pieces, pills. */
const RADII = new Set(["16px", "12px", "8px", "999px"]);
/** The type scale: 12 caption, 13 metadata, 15 body, 17 emphasized, 22 section title, 28 page title. */
const SIZES = new Set(["12px", "13px", "15px", "17px", "22px", "28px"]);
/** The 8pt grid, with 2 left for hairlines and optical nudges. */
const SPACES = new Set(["0", "2px", "4px", "8px", "12px", "16px", "24px", "32px", "40px", "48px"]);

type Tokens = Map<string, string>;

/** The two themes of `theme.css`: the default, which is dark, and the one under `[data-theme="light"]`. */
function themes(): { dark: Tokens; light: Tokens } {
  const dark: Tokens = new Map();
  const light: Tokens = new Map();
  for (const block of css.matchAll(/:root(\[data-theme="light"\])?\s*\{([^}]*)\}/g)) {
    const into = block[1] ? light : dark;
    const body = (block[2] ?? "").replace(/\/\*[\s\S]*?\*\//g, "");
    for (const [, name, value] of body.matchAll(/--([a-z0-9-]+):\s*([^;]+);/g)) {
      into.set(name ?? "", (value ?? "").trim());
    }
  }
  return { dark, light };
}

const { dark, light } = themes();
const bothThemes: [Tokens, string][] = [
  [dark, "dark"],
  [light, "light"],
];

/** The three channels and the alpha of a colour token, or null when it is not a plain colour. */
function channels(colour: string): [number, number, number, number] | null {
  const hex = /^#([0-9a-f]{6})$/i.exec(colour);
  if (hex) {
    const n = Number.parseInt(hex[1] ?? "", 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 1];
  }
  const rgb = /^rgb\(\s*(\d+)\s+(\d+)\s+(\d+)(?:\s*\/\s*([\d.]+)%)?\s*\)$/i.exec(colour);
  if (rgb) {
    return [Number(rgb[1]), Number(rgb[2]), Number(rgb[3]), rgb[4] ? Number(rgb[4]) / 100 : 1];
  }
  return null;
}

/** Follow the `var()` chain of a token to the colour it really draws. */
function resolve(token: string, theme: Tokens, depth = 0): string {
  const alias = /^var\(--([a-z0-9-]+)\)$/.exec(token);
  if (alias && depth < 8) return resolve(theme.get(alias[1] ?? "") ?? "", theme, depth + 1);
  return token;
}

/** The colour a translucent token really shows against a backdrop. */
function over(foreground: string, background: string, theme: Tokens): string {
  const top = resolve(foreground, theme);
  const under = resolve(background, theme);
  const fore = channels(top);
  const back = channels(under);
  if (!fore || !back || fore[3] >= 1) return top;
  const [fr, fg, fb, alpha] = fore;
  const [br, bg, bb] = back;
  const blend = (top: number, under: number) => Math.round(top * alpha + under * (1 - alpha));
  return `rgb(${blend(fr, br)} ${blend(fg, bg)} ${blend(fb, bb)})`;
}

function contrast(foreground: string, background: string): number {
  const luminance = (colour: string) => {
    const [r, g, b] = channels(colour) ?? [0, 0, 0, 1];
    const linear = (channel: number) => {
      const s = channel / 255;
      return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
  };
  const [a, b] = [luminance(foreground), luminance(background)];
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

/** Where text has to be readable: body and secondary on each surface, and status text on its own tint. */
const readable: [string, string][] = [
  ["text", "background"],
  ["text", "surface"],
  ["text", "surface-raised"],
  ["text", "hover"],
  ["text", "selected"],
  ["text-secondary", "background"],
  ["text-secondary", "surface"],
  ["text-secondary", "sidebar"],
  ["on-ink", "text"],
  ["progress-text", "background"],
  ["progress-text", "progress-fill"],
  ["attention-text", "background"],
  ["attention-text", "attention-fill"],
  ["attention-text", "attention-hover"],
  ["complete-text", "background"],
  ["complete-text", "complete-fill"],
  ["error-text", "background"],
  ["error-text", "error-fill"],
  ["error-text", "error-hover"],
  ["terminal-text", "terminal"],
  ["sidebar-group", "sidebar"],
  ["sidebar-text", "sidebar"],
  ["online-text", "sidebar"],
  ["text", "code-background"],
  ["text-secondary", "code-background"],
  ["code-keyword", "code-background"],
  ["code-string", "code-background"],
  ["code-number", "code-background"],
  ["complete-text", "code-background"],
  ["error-text", "code-background"],
];

/** Every file the app is built from, so a colour literal hidden in a component is found. */
function sources(dir: string, into: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    if (entry === "node_modules") continue;
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) sources(path, into);
    else if (/\.(css|tsx?|html)$/.test(entry)) into.push(path);
  }
  return into;
}

/**
 * Whether a token value holds a colour, so the themes are compared only where colour differs. Anything written as a
 * colour, or mixed or shaded out of another token, counts; a size, a radius or a duration does not.
 */
function isColour(value: string): boolean {
  return /^(?:#[0-9a-f]{3,8}|rgb\(|hsla?\(|color-mix\(|var\(--)/.test(value);
}

describe("the tokens", () => {
  it("define the same names in both themes", () => {
    expect(dark.size).toBeGreaterThan(80);
    const missing = [...dark.keys()].filter(
      (name) => /^(?:#[0-9a-f]{3,8}|rgb\()/.test(dark.get(name) ?? "") && !light.has(name),
    );
    const extra = [...light.keys()].filter(
      (name) => /^(?:#[0-9a-f]{3,8}|rgb\()/.test(light.get(name) ?? "") && !dark.has(name),
    );
    expect(missing, "missing from the light theme").toEqual([]);
    expect(extra, "missing from the dark theme").toEqual([]);
  });

  it("keeps the values of the standard", () => {
    const expected: [Tokens, string, string][] = [
      [dark, "background", "#121212"],
      [dark, "surface", "#1b1b1c"],
      [dark, "text", "#f4f3f1"],
      [dark, "text-secondary", "#a3a29e"],
      [dark, "progress", "#5c8bff"],
      [dark, "attention", "#f7b84b"],
      [dark, "complete", "#34c58a"],
      [dark, "error", "#ff6369"],
      [light, "background", "#fbfaf8"],
      [light, "surface", "#ffffff"],
      [light, "text", "#111111"],
      [light, "text-secondary", "#5e5e5b"],
      [light, "progress", "#2f6bff"],
      [light, "attention", "#f5a524"],
      [light, "complete", "#1fa971"],
      [light, "error", "#e5484d"],
    ];
    for (const [theme, name, value] of expected) {
      expect(theme.get(name), `--${name}`).toBe(value);
    }
  });

  it("radii are cards, controls, small pieces and pills", () => {
    const radii = [...css.matchAll(/--radius-[a-z0-9-]+:\s*([^;]+);/g)]
      .map((m) => (m[1] ?? "").trim())
      .filter((value) => !value.startsWith("var("));
    expect(radii.length).toBeGreaterThan(4);
    for (const radius of radii) expect(RADII.has(radius), radius).toBe(true);
  });

  it("spacing is on the 8pt grid", () => {
    const spaces = [...css.matchAll(/--space-[a-z0-9]+:\s*([^;]+);/g)].map((m) =>
      (m[1] ?? "").trim(),
    );
    expect(spaces.length).toBeGreaterThan(6);
    for (const space of spaces) expect(SPACES.has(space), space).toBe(true);
  });

  it("type utilities are the scale of the standard", () => {
    const sizes = [...tailwind.matchAll(/--text-[a-z0-9]+:\s*([^;]+);/g)].map((m) =>
      (m[1] ?? "").trim(),
    );
    expect(sizes.length).toBeGreaterThanOrEqual(7);
    for (const size of sizes) expect(SIZES.has(size), size).toBe(true);
    const heights = [...tailwind.matchAll(/--text-[a-z0-9]+--line-height:\s*([^;]+);/g)].map((m) =>
      Number(m[1]),
    );
    expect(heights.length).toBeGreaterThanOrEqual(7);
    for (const height of heights) expect(height).toBeGreaterThanOrEqual(1.1);
  });

  it("gives text 4.5 to 1 on what it sits on, in both themes", () => {
    for (const [theme, name] of bothThemes) {
      for (const [fore, back] of readable) {
        expect(theme.has(fore), `--${fore} in ${name}`).toBe(true);
        const backdrop = theme.get(back) ?? "";
        const ratio = contrast(over(theme.get(fore) ?? "", backdrop, theme), backdrop);
        expect(ratio, `--${fore} on --${back} in ${name}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("keeps muted below the readable text it must not compete with", () => {
    for (const [theme, label] of bothThemes) {
      for (const name of ["text-muted"]) {
        const background = theme.get("background") ?? "";
        const ratio = contrast(over(theme.get(name) ?? "", background, theme), background);
        expect(ratio, `--${name} in ${label}`).toBeLessThan(4.5);
        expect(ratio, `--${name} in ${label} should still be visible`).toBeGreaterThan(2.5);
      }
    }
  });

  it("puts no colour literal anywhere but theme.css", () => {
    const allowed = new Set(["theme.css"]);
    const offenders: string[] = [];
    for (const path of sources(srcRoot)) {
      const name = basename(path);
      if (allowed.has(name) || name.endsWith(".test.ts") || name.endsWith(".test.tsx")) continue;
      const text = readFileSync(path, "utf8");
      const hits = [
        ...new Set([
          ...(text.match(/#[0-9a-fA-F]{3,8}\b/g) ?? []),
          ...(text.match(/\brgba?\(/g) ?? []),
          ...(text.match(/\bhsla?\(/g) ?? []),
        ]),
      ];
      if (hits.length > 0) offenders.push(`${relative(srcRoot, path)}: ${hits.join(", ")}`);
    }
    expect(offenders).toEqual([]);
  });

  it("gives every colour token a Tailwind name", () => {
    const missing: string[] = [];
    for (const [theme, label] of bothThemes) {
      for (const [name, value] of theme) {
        if (!isColour(value)) continue;
        if (!tailwind.includes(`var(--${name})`)) missing.push(`${label}: --${name}`);
      }
    }
    expect(missing).toEqual([]);
  });

  it("asks for no token that is defined nowhere", () => {
    // A palette that has been replaced leaves declarations behind that name a token which no longer exists. The
    // browser drops those declarations without a word, so the only way to catch them is to read every name the app
    // asks for and check that one of the two themes, or the file itself, defines it.
    const defined = new Set<string>([...dark.keys(), ...light.keys()]);
    const offenders: string[] = [];
    for (const path of sources(srcRoot)) {
      const name = basename(path);
      if (name === "theme.css" || name.endsWith(".test.ts") || name.endsWith(".test.tsx")) continue;
      const text = readFileSync(path, "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
      const local = new Set([...text.matchAll(/--([a-z0-9-]+):/g)].map(([, own]) => own ?? ""));
      // A name the file builds at runtime is followed by something other than a bracket, and this cannot know what
      // name it comes to; every name written out in full it can.
      const asked = new Set(
        [...text.matchAll(/var\(--([a-z0-9-]+)\s*[,)]/g)].map(([, own]) => own ?? ""),
      );
      const dead = [...asked].filter((name1) => !defined.has(name1) && !local.has(name1));
      if (dead.length > 0) {
        const shown = dead.map((name1) => `--${name1}`).join(", ");
        offenders.push(`${relative(srcRoot, path)}: ${shown}`);
      }
    }
    expect(offenders).toEqual([]);
  });
});
