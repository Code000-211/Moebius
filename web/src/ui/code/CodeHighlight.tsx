/**
 * A block of code as the tokens its parser gives (see `ui/code/highlight.ts`), drawn as spans of token classes on the
 * code ground. The module is loaded lazily (React.lazy in `CodeBlock`), so the grammars it carries stay out of the
 * main bundle.
 */
import { useMemo } from "react";
import { highlight } from "./highlight";

export default function CodeHighlight({ code, language }: { code: string; language?: string }) {
  const tokens = useMemo(() => highlight(code, language), [code, language]);
  return (
    <code className="whitespace-pre">
      {tokens.map((token, place) =>
        token.classes ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: the tokens come for one code string, are replaced wholesale when it changes, and draw in order
          <span key={place} className={token.classes}>
            {token.text}
          </span>
        ) : (
          token.text
        ),
      )}
    </code>
  );
}
