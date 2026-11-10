/**
 * The shared highlighting of code: one palette from the code tokens of the theme, built twice — as CSS classes over
 * the tree a parser gives (`codeHighlighter`, for a static block of code) and as a live `HighlightStyle`
 * (`editHighlight`, for the editor) — so a keyword, a string and a number look the same wherever code is drawn. The
 * colours hold 4.5 to 1 on the code ground in both themes, which is why `--code-keyword`, `--code-string` and
 * `--code-number` exist rather than the category hues; `styles/highlight.css` styles the classes. The languages code
 * may come in by name are real Lezer grammars where the web keeps them (`@codemirror/lang-*`) and the legacy modes of
 * `@codemirror/legacy-modes` for the rest, Luau served by Lua. A name the map does not carry, or no name at all, is
 * plain code.
 */
import { cpp } from "@codemirror/lang-cpp";
import { css } from "@codemirror/lang-css";
import { html } from "@codemirror/lang-html";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { python } from "@codemirror/lang-python";
import { rust } from "@codemirror/lang-rust";
import { ensureSyntaxTree, HighlightStyle, StreamLanguage } from "@codemirror/language";
import { csharp, java, kotlin, scala } from "@codemirror/legacy-modes/mode/clike";
import { diff } from "@codemirror/legacy-modes/mode/diff";
import { dockerFile } from "@codemirror/legacy-modes/mode/dockerfile";
import { go } from "@codemirror/legacy-modes/mode/go";
import { lua } from "@codemirror/legacy-modes/mode/lua";
import { powerShell } from "@codemirror/legacy-modes/mode/powershell";
import { properties } from "@codemirror/legacy-modes/mode/properties";
import { r } from "@codemirror/legacy-modes/mode/r";
import { ruby } from "@codemirror/legacy-modes/mode/ruby";
import { shell } from "@codemirror/legacy-modes/mode/shell";
import { standardSQL } from "@codemirror/legacy-modes/mode/sql";
import { swift } from "@codemirror/legacy-modes/mode/swift";
import { toml } from "@codemirror/legacy-modes/mode/toml";
import { yaml } from "@codemirror/legacy-modes/mode/yaml";
import type { Extension } from "@codemirror/state";
import { EditorState } from "@codemirror/state";
import { highlightTree, type Tag, tagHighlighter, tags } from "@lezer/highlight";

/** A token style in the one palette: the tags it covers, its class, its colour, and the shape it takes when set. */
interface TokenStyle {
  tags: Tag | readonly Tag[];
  class: string;
  color: string;
  weight?: number;
  italic?: boolean;
}

/**
 * The palette, once: what the editor draws with `editHighlight` and a block of code with `codeHighlighter`, the same
 * five inks (keyword, string, number, secondary comment, semibold ink) the theme's code tokens carry.
 */
const TOKENS: readonly TokenStyle[] = [
  {
    tags: [
      tags.keyword,
      tags.controlKeyword,
      tags.moduleKeyword,
      tags.operatorKeyword,
      tags.definitionKeyword,
      tags.modifier,
      tags.self,
    ],
    class: "tok-keyword",
    color: "var(--code-keyword)",
  },
  {
    tags: [tags.string, tags.special(tags.string), tags.character],
    class: "tok-string",
    color: "var(--code-string)",
  },
  {
    tags: [tags.number, tags.bool, tags.null, tags.atom, tags.unit, tags.propertyName],
    class: "tok-number",
    color: "var(--code-number)",
  },
  { tags: tags.comment, class: "tok-comment", color: "var(--text-secondary)", italic: true },
  {
    tags: [
      tags.function(tags.variableName),
      tags.standard(tags.variableName),
      tags.className,
      tags.typeName,
      tags.tagName,
      tags.macroName,
    ],
    class: "tok-title",
    color: "var(--text)",
    weight: 600,
  },
  { tags: tags.inserted, class: "tok-add", color: "var(--complete-text)" },
  { tags: tags.deleted, class: "tok-del", color: "var(--error-text)" },
  { tags: tags.invalid, class: "tok-invalid", color: "var(--error-text)" },
];

/** The palette for the editor, as a `HighlightStyle` over the tags whose text it colours directly. */
export const editHighlight = HighlightStyle.define(
  TOKENS.map((token) => ({
    tag: token.tags,
    color: token.color,
    fontWeight: token.weight,
    fontStyle: token.italic ? ("italic" as const) : undefined,
  })),
);

/** The palette for a block of code already drawn, as classes a map of tags to CSS carries. */
const codeHighlighter = tagHighlighter(
  TOKENS.map((token) => ({ tag: token.tags, class: token.class })),
);

/** A piece of code and, when a token styles it, the highlight classes. */
export interface CodeToken {
  text: string;
  classes: string;
}

/** The languages a block of code may ask for, by the name it names, lowercase; any other is plain. */
const PARSERS: Record<string, Extension> = {
  bash: StreamLanguage.define(shell),
  sh: StreamLanguage.define(shell),
  shell: StreamLanguage.define(shell),
  zsh: StreamLanguage.define(shell),
  lua: StreamLanguage.define(lua),
  luau: StreamLanguage.define(lua),
  toml: StreamLanguage.define(toml),
  tml: StreamLanguage.define(toml),
  yaml: StreamLanguage.define(yaml),
  yml: StreamLanguage.define(yaml),
  diff: StreamLanguage.define(diff),
  patch: StreamLanguage.define(diff),
  dockerfile: StreamLanguage.define(dockerFile),
  docker: StreamLanguage.define(dockerFile),
  go: StreamLanguage.define(go),
  golang: StreamLanguage.define(go),
  csharp: StreamLanguage.define(csharp),
  cs: StreamLanguage.define(csharp),
  "c#": StreamLanguage.define(csharp),
  java: StreamLanguage.define(java),
  kotlin: StreamLanguage.define(kotlin),
  kt: StreamLanguage.define(kotlin),
  kts: StreamLanguage.define(kotlin),
  scala: StreamLanguage.define(scala),
  ruby: StreamLanguage.define(ruby),
  rb: StreamLanguage.define(ruby),
  swift: StreamLanguage.define(swift),
  sql: StreamLanguage.define(standardSQL),
  r: StreamLanguage.define(r),
  powershell: StreamLanguage.define(powerShell),
  ps1: StreamLanguage.define(powerShell),
  pwsh: StreamLanguage.define(powerShell),
  properties: StreamLanguage.define(properties),
  ini: StreamLanguage.define(properties),
  env: StreamLanguage.define(properties),
  conf: StreamLanguage.define(properties),
  cfg: StreamLanguage.define(properties),
  rust: rust(),
  javascript: javascript(),
  js: javascript(),
  mjs: javascript(),
  cjs: javascript(),
  jsx: javascript({ jsx: true }),
  typescript: javascript({ typescript: true }),
  ts: javascript({ typescript: true }),
  mts: javascript({ typescript: true }),
  cts: javascript({ typescript: true }),
  tsx: javascript({ typescript: true, jsx: true }),
  python: python(),
  py: python(),
  python2: python(),
  python3: python(),
  json: json(),
  jsonc: json(),
  json5: json(),
  c: cpp(),
  cpp: cpp(),
  cc: cpp(),
  cxx: cpp(),
  h: cpp(),
  hpp: cpp(),
  hh: cpp(),
  css: css(),
  scss: css(),
  less: css(),
  html: html(),
  xhtml: html(),
  xml: html(),
  svg: html(),
  markdown: markdown(),
  md: markdown(),
};

/** The name a language asks for, normalized: `TOML` and `toml` are one. */
function nameOf(language?: string): string {
  return (language ?? "")
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9+#]/g, "");
}

/** The extension the language's parser comes as, null for a name the map does not carry (plain code). */
export function parserOf(language?: string): Extension | null {
  return PARSERS[nameOf(language)] ?? null;
}

/**
 * The tokens of a block of code: the parser of its language over the whole text, the palette's classes over the
 * tree, the plain in-between kept. A block is small, so the parse is quick; the budget of a second keeps a grammar
 * that does not finish within its turn from holding the page, at the cost of drawing the block plain.
 */
export function highlight(code: string, language?: string): CodeToken[] {
  const parser = parserOf(language);
  if (!parser) return [{ text: code, classes: "" }];
  const state = EditorState.create({ doc: code, extensions: [parser] });
  const tree = ensureSyntaxTree(state, code.length, 1000);
  if (!tree) return [{ text: code, classes: "" }];
  const tokens: CodeToken[] = [];
  let at = 0;
  highlightTree(tree, codeHighlighter, (from, to, classes) => {
    if (from > at) tokens.push({ text: code.slice(at, from), classes: "" });
    tokens.push({ text: code.slice(from, to), classes });
    at = to;
  });
  if (at < code.length) tokens.push({ text: code.slice(at), classes: "" });
  return tokens;
}
