/**
 * The agent's and the person's text as Markdown (GitHub flavoured: tables, task lists, strikethrough), drawn in the
 * app's colours. Code blocks are highlighted by highlight.js through `rehype-highlight` (its common languages) and
 * have a Copy button. Links open in a new tab. Raw HTML in the text is not rendered, as `react-markdown` does by
 * default.
 */
import type { Element, ElementContent } from "hast";
import { memo, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import rehypeHighlight from "rehype-highlight";
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
  pre({ node, children }) {
    const code = node?.children.find(
      (child): child is Element => child.type === "element" && child.tagName === "code",
    );
    return (
      <CodeBlock language={languageOf(code)} code={code ? textOf(code).replace(/\n$/, "") : ""}>
        {children}
      </CodeBlock>
    );
  },
  code({ className, children }) {
    // A block's `code` is inside `pre`; this is the inline one (or the block's own element, which keeps its classes).
    const block = typeof className === "string" && className.includes("language-");
    return (
      <code
        className={cn(
          block ? className : "rounded-sm bg-card px-half font-mono text-[0.9em] text-high",
        )}
      >
        {children}
      </code>
    );
  },
  a({ href, children }) {
    return (
      <a href={href} target="_blank" rel="noopener noreferrer" className="text-accent underline">
        {children}
      </a>
    );
  },
  p: ({ children }) => <p className="my-base first:mt-0 last:mb-0">{children}</p>,
  ul: ({ children, className }) => (
    <ul
      className={cn(
        "my-base list-disc pl-6",
        className?.includes("contains-task-list") && "list-none pl-1",
      )}
    >
      {children}
    </ul>
  ),
  ol: ({ children }) => <ol className="my-base list-decimal pl-6">{children}</ol>,
  h1: ({ children }) => (
    <h1 className="mb-base mt-double text-lg font-bold text-high">{children}</h1>
  ),
  h2: ({ children }) => (
    <h2 className="mb-base mt-double text-base font-bold text-high">{children}</h2>
  ),
  h3: ({ children }) => (
    <h3 className="mb-base mt-plusfifty text-sm font-bold text-high">{children}</h3>
  ),
  h4: ({ children }) => (
    <h4 className="mb-base mt-plusfifty text-sm font-bold text-high">{children}</h4>
  ),
  blockquote: ({ children }) => (
    <blockquote className="my-base border-l-2 pl-plusfifty text-low">{children}</blockquote>
  ),
  table: ({ children }) => (
    <div className="my-base overflow-x-auto">
      <table className="min-w-full border-collapse text-sm">{children}</table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border px-base py-half text-left font-bold text-high">{children}</th>
  ),
  td: ({ children }) => <td className="border px-base py-half">{children}</td>,
  hr: () => <hr className="my-double" />,
};

const REMARK = [remarkGfm];
// `detect` finds the language of a block that names none; `ignoreMissing` keeps an unknown name from throwing.
const REHYPE = [[rehypeHighlight, { detect: false, ignoreMissing: true }]] as never;

function MarkdownText({ content, className }: { content: string; className?: string }): ReactNode {
  return (
    <div className={cn("text-sm text-normal", className)}>
      <ReactMarkdown remarkPlugins={REMARK} rehypePlugins={REHYPE} components={components}>
        {content}
      </ReactMarkdown>
    </div>
  );
}

/** Memoised, so that a conversation redraws only the text that changed while an answer streams in. */
export const Markdown = memo(MarkdownText);
