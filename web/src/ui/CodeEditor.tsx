/**
 * A code editor over CodeMirror 6 (MIT) with Lua highlighting, for the column scripts (Luau is Lua with types, and the
 * highlighting of Lua serves it). Written for Moebius, not copied. It is controlled by `value` and `onChange`, and can
 * mark one line as wrong (`problem`) with a gutter marker, an underline and the message on hover. The editor is heavy,
 * so a page loads this module lazily (`React.lazy`) and the main bundle does not grow.
 */
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import {
  bracketMatching,
  HighlightStyle,
  indentOnInput,
  StreamLanguage,
  syntaxHighlighting,
} from "@codemirror/language";
import { lua } from "@codemirror/legacy-modes/mode/lua";
import { lintGutter, setDiagnostics } from "@codemirror/lint";
import { EditorState } from "@codemirror/state";
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { useEffect, useRef } from "react";

export interface CodeProblem {
  /** The line the check named, counted from 1. */
  line: number;
  message: string;
}

/**
 * The colours of the tokens: the ones `styles/highlight.css` gives the chat's code blocks, so a keyword, a string, a
 * number and a comment look the same here as they do in an answer. Each holds 4.5 to 1 on the code ground in both
 * themes, which is why the code tokens exist rather than the category hues.
 */
const highlight = HighlightStyle.define([
  { tag: [tags.keyword, tags.operatorKeyword, tags.controlKeyword], color: "var(--code-keyword)" },
  { tag: [tags.string, tags.special(tags.string)], color: "var(--code-string)" },
  { tag: [tags.number, tags.bool, tags.null, tags.atom], color: "var(--code-number)" },
  {
    tag: [tags.comment, tags.lineComment, tags.blockComment],
    color: "var(--text-secondary)",
    fontStyle: "italic",
  },
  // What the chat draws with `.hljs-title`: the ink of the page, semibold, so a call stands out without a hue.
  {
    tag: [tags.function(tags.variableName), tags.standard(tags.variableName)],
    color: "var(--text)",
    fontWeight: 600,
  },
]);

const theme = EditorView.theme(
  {
    "&": {
      backgroundColor: "var(--code-background)",
      color: "var(--text)",
      fontSize: "var(--size-code)",
      border: "1px solid var(--hairline)",
      borderRadius: "var(--radius-control)",
    },
    // The ring of a field, on the whole editor so the gutters are inside it. The app-wide `:focus-visible` rule would
    // otherwise draw a second one around the text alone, which is clipped when the script is longer than the window.
    "&.cm-focused": { outline: "2px solid var(--focus)", outlineOffset: "2px" },
    ".cm-content:focus-visible": { outline: "none" },
    ".cm-scroller": {
      fontFamily: "var(--font-mono)",
      lineHeight: "1.6",
      minHeight: "320px",
      maxHeight: "70vh",
    },
    ".cm-content": { caretColor: "var(--text)" },
    ".cm-cursor": { borderLeftColor: "var(--text)" },
    ".cm-gutters": {
      backgroundColor: "var(--surface-sunken)",
      color: "var(--text-secondary)",
      border: "none",
      borderRadius: "var(--radius-control) 0 0 var(--radius-control)",
    },
    ".cm-activeLine": { backgroundColor: "var(--hover)" },
    ".cm-activeLineGutter": { backgroundColor: "var(--hover)" },
    // The selected text: a tint of the ink, so it shows on the code ground in either theme, where `--selected` is too
    // close to the ground to see in the light one.
    ".cm-selectionBackground": { backgroundColor: "var(--hairline-strong)" },
    "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground": {
      backgroundColor: "var(--hairline-strong)",
    },
    ".cm-lintRange-error": {
      backgroundImage: "none",
      textDecoration: "underline wavy var(--error)",
    },
    ".cm-tooltip": {
      backgroundColor: "var(--surface)",
      color: "var(--text)",
      border: "1px solid var(--hairline-strong)",
    },
  },
  { dark: true },
);

export default function CodeEditor({
  value,
  onChange,
  problem,
  label,
}: {
  value: string;
  onChange: (text: string) => void;
  problem?: CodeProblem | null;
  /** Names the editor for a screen reader. */
  label: string;
}) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  // The editor is made once, with the text it is first given; `value` after that is applied by the effect below.
  const first = useRef({ value, label });
  onChangeRef.current = onChange;

  useEffect(() => {
    if (!host.current) return;
    const editor = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: first.current.value,
        extensions: [
          lineNumbers(),
          highlightActiveLineGutter(),
          highlightActiveLine(),
          drawSelection(),
          history(),
          indentOnInput(),
          bracketMatching(),
          lintGutter(),
          StreamLanguage.define(lua),
          syntaxHighlighting(highlight),
          keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
          theme,
          EditorView.contentAttributes.of({
            "aria-label": first.current.label,
            "aria-multiline": "true",
          }),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) onChangeRef.current(update.state.doc.toString());
          }),
        ],
      }),
    });
    view.current = editor;
    return () => {
      editor.destroy();
      view.current = null;
    };
  }, []);

  useEffect(() => {
    const editor = view.current;
    if (editor && editor.state.doc.toString() !== value) {
      editor.dispatch({ changes: { from: 0, to: editor.state.doc.length, insert: value } });
    }
  }, [value]);

  useEffect(() => {
    const editor = view.current;
    if (!editor) return;
    const doc = editor.state.doc;
    const diagnostics = problem
      ? (() => {
          const line = doc.line(Math.min(Math.max(problem.line, 1), doc.lines));
          return [
            {
              from: line.from,
              to: Math.max(line.to, line.from),
              severity: "error" as const,
              message: problem.message,
            },
          ];
        })()
      : [];
    editor.dispatch(setDiagnostics(editor.state, diagnostics));
  }, [problem]);

  return <div ref={host} data-testid="code-editor" />;
}
