// @vitest-environment node
/**
 * Keeps the tokens of `theme.css` and `tailwind.css` inside the design standard,
 * [`docs/design-web.md`](../../../docs/design-web.md). It parses the token table of that page and compares every
 * value with `theme.css`, in both themes, so a value that differs in either place fails; it checks the radii, the
 * depth tokens, the tabular numerals, the contrast pairs the standard names and the muted range, that no colour
 * literal appears outside `theme.css`, and that no name the app asks for is defined nowhere, because a declaration
 * that names a token which is not there is dropped in silence and the control simply loses its colour.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { basename, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("./theme.css", import.meta.url), "utf8");
const tailwind = readFileSync(new URL("./tailwind.css", import.meta.url), "utf8");
const srcRoot = fileURLToPath(new URL("..", import.meta.url));
const standard = readFileSync(new URL("../../../docs/design-web.md", import.meta.url), "utf8");

/** The radii the standard allows: 6 for controls and rows, 8 for cards, 10 for the panel and windows, round for dots. */
const RADII = new Set(["6px", "8px", "10px", "9999px"]);
/**
 * The type scale: 11 for keys and small capitals, 12 for captions and metadata, 13 body, 14 the text of an issue or a
 * page, and titles of 15, 17 and 20.
 */
const SIZES = new Set(["11px", "12px", "13px", "14px", "15px", "17px", "20px"]);
/** A 4px grid, with 2, 6 and 10 for the tight places of a dense row. */
const SPACES = new Set([
  "0",
  "2px",
  "4px",
  "6px",
  "8px",
  "10px",
  "12px",
  "16px",
  "20px",
  "24px",
  "32px",
  "40px",
  "48px",
]);
/** The heights of the standard: a row of the sidebar, a control, a row of a list, the head bar of a page. */
const HEIGHTS: [string, string][] = [
  ["row-sidebar", "28px"],
  ["row-control", "32px"],
  ["row-list", "36px"],
  ["head-bar", "44px"],
];
/** The depth tokens of the former palette, which the new standard flattens rather than deletes. */
const DEPTH = [
  "edge",
  "shadow-card",
  "shadow-card-hover",
  "shadow-raised",
  "shadow-relief",
  "glow",
];

type Tokens = Map<string, string>;

/**
 * The two themes of `theme.css`: light is the base block and dark the `[data-theme="dark"]` block, which sets only
 * what differs, so the dark theme is the base with that block over it.
 */
function themes(): { dark: Tokens; light: Tokens } {
  const light: Tokens = new Map();
  const over: Tokens = new Map();
  for (const block of css.matchAll(/:root(\[data-theme="dark"\])?\s*\{([^}]*)\}/g)) {
    const into = block[1] ? over : light;
    const body = (block[2] ?? "").replace(/\/\*[\s\S]*?\*\//g, "");
    for (const [, name, value] of body.matchAll(/--([a-z0-9-]+):\s*([^;]+);/g)) {
      into.set(name ?? "", (value ?? "").trim());
    }
  }
  return { light, dark: new Map([...light, ...over]) };
}

/** One row of the standard's token table: the name and its two values, as written. */
function tableOfTheStandard(): { name: string; light: string; dark: string }[] {
  const rows: { name: string; light: string; dark: string }[] = [];
  const start = standard.indexOf("### Token table");
  const end = standard.indexOf("The size table, which follows");
  expect(start, "the token table's heading").toBeGreaterThan(-1);
  expect(end, "the end of the token table").toBeGreaterThan(start);
  for (const line of standard.slice(start, end).split("\n")) {
    const cells = /^\|\s*`--([a-z0-9-]+)`\s*\|\s*(.+?)\s*\|\s*(.+?)\s*\|[^|]*\|\s*$/.exec(line);
    if (cells) rows.push({ name: cells[1] ?? "", light: cells[2] ?? "", dark: cells[3] ?? "" });
  }
  return rows;
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
  // `color-mix(in srgb, <one> N%, <other>)`, as the faint tints of the former names are written.
  const mix =
    /^color-mix\(in srgb,\s*(var\(--[a-z0-9-]+\))\s+(\d+)%,\s*(var\(--[a-z0-9-]+\))\)$/.exec(token);
  if (mix && depth < 8) {
    const top = channels(resolve(mix[1] ?? "", theme, depth + 1));
    const under = channels(resolve(mix[3] ?? "", theme, depth + 1));
    if (top && under) {
      const share = Number(mix[2]) / 100;
      const blend = (a: number, b: number) => Math.round(a * share + b * (1 - share));
      return `rgb(${blend(top[0], under[0])} ${blend(top[1], under[1])} ${blend(top[2], under[2])})`;
    }
  }
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

/** The surfaces the standard names for body, secondary, muted and indigo text. */
const SURFACES = ["panel", "canvas", "accent-tint", "row-hover", "group-fill", "control-fill"];

/** The white label of the primary button and of an avatar. */
const WHITE = "#ffffff";

/** Where text has to be readable, as the standard lists the pairs: foreground, background. */
const readable: [string, string][] = [
  ...["text", "text-secondary", "text-muted", "accent-text"].flatMap((fore): [string, string][] =>
    SURFACES.map((back): [string, string] => [fore, back]),
  ),
  ["needs-you-text", "panel"],
  ["running-text", "panel"],
  ["danger-text", "panel"],
  ["danger-text", "canvas"],
  ["danger-text", "row-hover"],
  ["needs-you-text", "needs-you-tint"],
  [WHITE, "accent-fill"],
  [WHITE, "avatar-1"],
  [WHITE, "avatar-2"],
  [WHITE, "avatar-3"],
  ["code-text", "code-bg"],
  ["code-head", "code-bg"],
  ["code-keyword", "code-bg"],
  ["code-function", "code-bg"],
  ["code-operator", "code-bg"],
  ["code-number", "code-bg"],
  ["code-comment", "code-bg"],
  // What the former names still draw, on what they are drawn on.
  ["text", "selected"],
  ["text-secondary", "sidebar"],
  ["text-secondary", "selected"],
  ["sidebar-group", "sidebar"],
  ["sidebar-text", "sidebar"],
  ["online-text", "sidebar"],
  ["progress-text", "progress-fill"],
  ["attention-text", "attention-fill"],
  ["complete-text", "complete-fill"],
  ["error-text", "error-fill"],
  ["error-text", "error-hover"],
  ["terminal-text", "terminal"],
  ["on-ink", "text"],
];

/** The marks that carry no words, which need 3 to 1 on the panel. */
const MARKS = [
  "needs-you",
  "running",
  "danger",
  "status-todo",
  "status-done",
  "status-review",
  "text-faint",
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
  it("takes every value of the standard's token table, in both themes", () => {
    const table = tableOfTheStandard();
    expect(table.length, "rows of the token table").toBeGreaterThan(40);
    const wrong: string[] = [];
    for (const row of table) {
      for (const [theme, label, expected] of [
        [light, "light", row.light],
        [dark, "dark", row.dark],
      ] as [Tokens, string, string][]) {
        const value = theme.get(row.name);
        // A colour is compared without regard to the case of its hex digits; any other value as written.
        const same =
          value !== undefined &&
          (/^#[0-9a-f]{6}$/i.test(expected)
            ? value.toLowerCase() === expected.toLowerCase()
            : value === expected);
        if (!same)
          wrong.push(`--${row.name} in ${label}: theme.css has ${value}, the table ${expected}`);
      }
    }
    expect(wrong).toEqual([]);
  });

  it("makes light the base and lets dark set only names the base has", () => {
    const base = css.indexOf(":root {");
    const overrides = css.indexOf(':root[data-theme="dark"] {');
    expect(base, "the base block").toBeGreaterThan(-1);
    expect(overrides, "the dark block").toBeGreaterThan(base);
    expect(light.get("canvas")).toBe("#f6f6f8");
    expect(dark.get("canvas")).toBe("#0f0f10");
    const body = css.slice(overrides).replace(/\/\*[\s\S]*?\*\//g, "");
    const only = [...body.matchAll(/--([a-z0-9-]+):/g)].map(([, name]) => name ?? "");
    expect(only.filter((name) => !light.has(name))).toEqual([]);
    // Every colour of the table differs, or is the same on purpose in the standard itself: the dark block sets each.
    for (const row of tableOfTheStandard())
      expect(only, `--${row.name} in the dark block`).toContain(row.name);
  });

  it("flattens depth: a shadow is one hairline or nothing, and there is no grain and no glow", () => {
    const flat =
      /^(?:0 0 0 0 transparent|0 0 0 1px var\(--hairline(?:-strong)?\)|var\(--shadow-window\))$/;
    for (const name of DEPTH) {
      for (const [theme, label] of bothThemes) {
        expect(theme.get(name), `--${name} in ${label}`).toMatch(flat);
      }
    }
    for (const [theme, label] of bothThemes) {
      expect(theme.get("grain"), `--grain in ${label}`).toBe("0");
      expect(theme.get("glow"), `--glow in ${label}`).toBe("0 0 0 0 transparent");
      expect(theme.get("wash"), `--wash in ${label}`).toBe("transparent");
    }
    // The one real shadow is the window's, and it has one blur.
    for (const [theme, label] of bothThemes) {
      const blurs = (theme.get("shadow-window") ?? "").match(/\d+px 32px/g) ?? [];
      expect(blurs.length, `--shadow-window in ${label}`).toBe(1);
    }
  });

  it("sets tabular numerals on the body and follows the theme with color-scheme", () => {
    const body = /\n {2}body \{([^}]*)\}/.exec(tailwind)?.[1] ?? "";
    expect(body).toContain("font-variant-numeric: tabular-nums;");
    expect(tailwind).toMatch(/html \{[^}]*color-scheme: light;/);
    expect(tailwind).toMatch(/html\[data-theme="dark"\] \{[^}]*color-scheme: dark;/);
    expect(dark.get("canvas")).not.toBe(light.get("canvas"));
  });

  it("keeps the heights of rows and of the head bar", () => {
    for (const [name, value] of HEIGHTS) expect(dark.get(name), `--${name}`).toBe(value);
  });

  it("radii are controls, cards, the panel and dots", () => {
    const radii = [...css.matchAll(/--radius-[a-z0-9-]+:\s*([^;]+);/g)]
      .map((m) => (m[1] ?? "").trim())
      .filter((value) => !value.startsWith("var("));
    expect(radii.length).toBeGreaterThan(4);
    for (const radius of radii) expect(RADII.has(radius), radius).toBe(true);
    expect(css).toContain("--radius-control: 6px;");
    expect(css).toContain("--radius-card: 8px;");
    expect(css).toContain("--radius-panel: 10px;");
    expect(css).toContain("--radius-round: 9999px;");
  });

  it("spacing is on the grid", () => {
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
    const named = [...css.matchAll(/--size-[a-z-]+:\s*([^;]+);/g)].map((m) => (m[1] ?? "").trim());
    expect(named.length).toBeGreaterThanOrEqual(7);
    for (const size of named) expect(SIZES.has(size), size).toBe(true);
    expect(dark.get("size-body"), "body text").toBe("13px");
    expect(dark.get("size-issue"), "the text of an issue").toBe("14px");
  });

  it("turns every transition off for reduced motion", () => {
    const block =
      /@media \(prefers-reduced-motion: reduce\) \{\s*\*,[^}]*\}/.exec(tailwind)?.[0] ?? "";
    expect(block).toContain("transition: none;");
    expect(block).toContain("animation-duration: 1ms;");
    const attribute = /:root\[data-reduce-motion="true"\] \*,[^}]*\}/.exec(tailwind)?.[0] ?? "";
    expect(attribute).toContain("transition: none;");
    expect(attribute).toContain("animation-duration: 1ms;");
  });

  it("has no lift utility, no running-dot keyframes and no motion over 120ms", () => {
    expect(tailwind).not.toContain("@utility lift");
    expect(tailwind).not.toContain("running-dot");
    for (const [, ms] of css.matchAll(/--motion-[a-z]+: (\d+)ms/g))
      expect(Number(ms)).toBeLessThanOrEqual(120);
  });

  it("gives text 4.5 to 1 on what it sits on, in both themes", () => {
    for (const [theme, name] of bothThemes) {
      for (const [fore, back] of readable) {
        const foreground = fore.startsWith("#") ? fore : (theme.get(fore) ?? "");
        expect(foreground, `--${fore} in ${name}`).not.toBe("");
        const backdrop = resolve(theme.get(back) ?? "", theme);
        const ratio = contrast(over(foreground, backdrop, theme), backdrop);
        expect(ratio, `--${fore} on --${back} in ${name}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("holds the muted range: muted is text at 4.5 to 1, faint is a mark that never carries words", () => {
    for (const [theme, label] of bothThemes) {
      for (const surface of SURFACES) {
        const background = theme.get(surface) ?? "";
        const muted = contrast(theme.get("text-muted") ?? "", background);
        expect(muted, `--text-muted on --${surface} in ${label}`).toBeGreaterThanOrEqual(4.5);
        const faint = contrast(theme.get("text-faint") ?? "", background);
        expect(faint, `--text-faint on --${surface} in ${label}`).toBeLessThan(4.5);
      }
      const panel = theme.get("panel") ?? "";
      for (const name of MARKS) {
        const ratio = contrast(theme.get(name) ?? "", panel);
        expect(ratio, `--${name} on --panel in ${label}`).toBeGreaterThanOrEqual(3);
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
