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

/** The colours of the tokens: the same Deep Velvet ones the chat's code blocks use. */
const highlight = HighlightStyle.define([
  { tag: [tags.keyword, tags.operatorKeyword, tags.controlKeyword], color: "var(--accent)" },
  { tag: [tags.string, tags.special(tags.string)], color: "var(--teal)" },
  { tag: [tags.number, tags.bool, tags.null, tags.atom], color: "var(--amber)" },
  {
    tag: [tags.comment, tags.lineComment, tags.blockComment],
    color: "var(--text-muted)",
    fontStyle: "italic",
  },
  {
    tag: [tags.function(tags.variableName), tags.standard(tags.variableName)],
    color: "var(--live)",
  },
]);

const theme = EditorView.theme(
  {
    "&": {
      backgroundColor: "var(--recessed)",
      color: "var(--text)",
      fontSize: "12px",
      border: "1px solid var(--line)",
      borderRadius: "12px",
    },
    "&.cm-focused": { outline: "none", borderColor: "var(--accent)" },
    ".cm-scroller": {
      fontFamily: "var(--font-mono)",
      lineHeight: "1.6",
      minHeight: "320px",
      maxHeight: "70vh",
    },
    ".cm-content": { caretColor: "var(--text)" },
    ".cm-cursor": { borderLeftColor: "var(--text)" },
    ".cm-gutters": {
      backgroundColor: "var(--recessed)",
      color: "var(--text-muted)",
      border: "none",
      borderRadius: "12px 0 0 12px",
    },
    ".cm-activeLine": { backgroundColor: "var(--card)" },
    ".cm-activeLineGutter": { backgroundColor: "var(--card)" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": {
      backgroundColor: "var(--accent-fill)",
    },
    ".cm-lintRange-error": { backgroundImage: "none", textDecoration: "underline wavy var(--red)" },
    ".cm-tooltip": {
      backgroundColor: "var(--surface)",
      color: "var(--text)",
      border: "1px solid var(--line-strong)",
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
