/**
 * The agent's and the person's text as Markdown (GitHub flavoured: tables, task lists, strikethrough), drawn in the
 * app's colours. It is body text: 15px on a relaxed line, headings from the scale rather than shrunk headings, and
 * monospace only where a character counts — a path, a command, a block of code. A heading is drawn one level under
 * the Markdown that asks for it, so a page keeps its one `<h1>` and an agent's `#` lands as an `<h2>`. Code blocks
 * are highlighted by the grammars of `ui/code/highlight.ts` (see `CodeBlock`) and have a Copy button. What an agent
 * writes line by line is drawn line by line (`keepNewlines`); a Markdown file's own wrapping stays soft. Links open
 * in a new tab. Raw HTML in the text is not rendered, as `react-markdown` does by default.
 */
import { CheckSquareIcon, SquareIcon } from "@phosphor-icons/react";
import type { Element, ElementContent } from "hast";
import { memo, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { cn } from "../lib/cn";
import { CodeBlock } from "./CodeBlock";

/** The plain text under a node of the syntax tree. */
export function textOf(node: Element | ElementContent): string {
  if (node.type === "text") return node.value;
  if (node.type === "element") return node.children.map(textOf).join("");
  return "";
}

/** The language of a `<code>` element, from its `language-xxx` class. */
export function languageOf(node: Element | undefined): string | undefined {
  const classes = node?.properties?.className;
  if (!Array.isArray(classes)) return undefined;
  const found = classes.find((name) => typeof name === "string" && name.startsWith("language-"));
  return typeof found === "string" ? found.slice("language-".length) : undefined;
}

const components: Components = {
  pre({ node }) {
    const code = node?.children.find(
      (child): child is Element => child.type === "element" && child.tagName === "code",
    );
    return (
      <CodeBlock language={languageOf(code)} code={code ? textOf(code).replace(/\n$/, "") : ""} />
    );
  },
  code({ className, children }) {
    // A block's `code` is drawn by `pre` above; `children` is built and dropped, so it renders only inline.
    if (typeof className === "string" && className.includes("language-")) return null;
    return <code className="rounded-small bg-code px-2 font-mono text-sm">{children}</code>;
  },
  a({ href, children }) {
    return (
      <a
        href={href}
        target="_blank"
        rel="noopener noreferrer"
        className="text-text underline underline-offset-4 transition-colors duration-(--motion-routine) ease-out hover:text-secondary"
      >
        {children}
      </a>
    );
  },
  p: ({ children }) => <p className="my-4 first:mt-0 last:mb-0">{children}</p>,
  ul: ({ children, className }) => (
    <ul
      className={cn(
        "my-4 list-disc pl-6",
        className?.includes("contains-task-list") && "list-none pl-0",
      )}
    >
      {children}
    </ul>
  ),
  ol: ({ children }) => <ol className="my-4 list-decimal pl-6">{children}</ol>,
  li: ({ children, className }) => (
    <li className={cn(className?.includes("task-list-item") && "flex items-start gap-2")}>
      {children}
    </li>
  ),
  // A box of a task list is a state of a line of text, not a control to press, so it is an icon with a word.
  input: ({ checked }) =>
    checked ? (
      <CheckSquareIcon
        role="img"
        aria-label="Done"
        className="mt-1 size-4 shrink-0 text-complete"
      />
    ) : (
      <SquareIcon
        role="img"
        aria-label="Not done"
        className="mt-1 size-4 shrink-0 text-secondary"
      />
    ),
  // One level down from what the Markdown asks for, and the same size each level was already drawn at: the outline of
  // the page stays page title, then the agent's sections.
  h1: ({ children }) => <h2 className="mb-2 mt-6 text-xl font-semibold text-text">{children}</h2>,
  h2: ({ children }) => <h3 className="mb-2 mt-6 text-lg font-semibold text-text">{children}</h3>,
  h3: ({ children }) => <h4 className="mb-2 mt-4 text-base font-semibold text-text">{children}</h4>,
  h4: ({ children }) => <h5 className="mb-2 mt-4 text-base font-semibold text-text">{children}</h5>,
  blockquote: ({ children }) => (
    <blockquote className="my-4 border-l-2 border-hairline-strong pl-4 text-secondary">
      {children}
    </blockquote>
  ),
  table: ({ children }) => (
    <div className="my-4 overflow-x-auto rounded-control">
      <table className="w-full border-collapse text-left text-base">{children}</table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border-b border-hairline bg-sunken px-3 py-2 text-left text-sm font-semibold text-secondary">
      {children}
    </th>
  ),
  td: ({ children }) => (
    <td className="border-b border-hairline px-3 py-2 align-top text-text">{children}</td>
  ),
  hr: () => <hr className="my-6 border-hairline" />,
};

const REMARK = [remarkGfm];

function MarkdownText({
  content,
  className,
  keepNewlines,
}: {
  content: string;
  className?: string;
  /**
   * Keep every newline the text was written with inside a paragraph and a list item. What an agent writes is a line
   * per thought, so its breaks are content: drawn as spaces they are lost from the screen, and — because a selection
   * copies what is drawn — from anything pasted out of it. A Markdown file is written the other way, hard-wrapped by
   * whoever wrote it, so there the breaks stay soft and only the blank lines make a paragraph.
   */
  keepNewlines?: boolean;
}): ReactNode {
  return (
    <div
      className={cn(
        "text-base leading-relaxed text-text",
        keepNewlines && "[&_li]:whitespace-pre-wrap [&_p]:whitespace-pre-wrap",
        className,
      )}
    >
      <ReactMarkdown remarkPlugins={REMARK} components={components}>
        {content}
      </ReactMarkdown>
    </div>
  );
}

/** Memoised, so that a conversation redraws only the text that changed while an answer streams in. */
export const Markdown = memo(MarkdownText);
