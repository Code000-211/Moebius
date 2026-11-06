/**
 * One turn of a conversation (the former desktop app's CH3-D): the person's message as its head and what the agent did and
 * answered as its body. Presentational: the turn comes in as data (see `chat/turns.ts`), and so do the folded state
 * and the clicks. A later page that shows a run's conversation draws it with this.
 *
 * The answer is body text on a readable measure. What the agent did is one row apiece — thinking, a tool, a group of
 * reads, a changed file, a todo list — each a control 40px tall that says its state in a word, opens to the text it
 * worked with, and keeps that text on the colour of code rather than in a box of its own colours.
 */
import { CaretRightIcon, CircleNotchIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import {
  inputPath,
  inputSummary,
  refused,
  type ToolCall,
  toolTitle,
  verdict,
} from "../../chat/conversation";
import type { Block, StepView, Turn as TurnData } from "../../chat/turns";
import { cn } from "../lib/cn";
import { plural } from "../lib/format";
import { ChatAggregatedToolEntries } from "./ChatAggregatedToolEntries";
import { ChatAssistantMessage } from "./ChatAssistantMessage";
import { ChatCollapsedThinking } from "./ChatCollapsedThinking";
import { ChatErrorMessage } from "./ChatErrorMessage";
import { ChatFileEntry } from "./ChatFileEntry";
import { ChatQuestion } from "./ChatQuestion";
import { ChatTodoList } from "./ChatTodoList";
import { ChatToolSummary } from "./ChatToolSummary";
import { ChatUserMessage } from "./ChatUserMessage";
import { CopyButton } from "./CopyButton";
import { DiffBody, type DiffChange, diffStats } from "./DiffBody";
import { Markdown } from "./Markdown";
import type { ToolStatusLike } from "./ToolStatusDot";

/** The most lines of a tool's output drawn; the rest is said to be left out. */
const MAX_OUTPUT_LINES = 200;

/** The status a tool call is in, told by one of the words `ToolStatusDot` knows. */
export function toolStatus(call: ToolCall): ToolStatusLike {
  if (refused(call)) return { status: "denied" };
  if (call.execution?.state === "finished") {
    return { status: call.execution.failed ? "failed" : "success" };
  }
  return { status: "created" };
}

/** The changes a file entry shows: an edit's replacements, or a written file as all added. */
function changesOf(call: ToolCall): DiffChange[] | null {
  const input = call.input;
  if (input.type === "edit") return input.changes;
  if (input.type === "write") return [{ oldText: "", newText: input.content }];
  return null;
}

/** What a tool was given and what it printed, on the colour of code, each with a Copy button. */
function Details({ call }: { call: ToolCall }) {
  const check = verdict(call);
  const output = call.output.split("\n");
  const shown = output.slice(0, MAX_OUTPUT_LINES).join("\n");
  const input =
    call.input.type === "command"
      ? call.input.command
      : call.input.type === "other"
        ? call.input.json
        : (inputPath(call.input) ?? inputSummary(call.input));
  return (
    <div
      data-testid="tool-details"
      className="reveal-copy mt-1 flex flex-col gap-3 rounded-control bg-code p-3"
    >
      {check && !check.allowed && (
        <p className="break-words text-sm font-semibold text-error-text">
          Blocked: {check.reason || "a rule refused this call"}
        </p>
      )}
      {input && (
        <div className="flex items-start gap-3">
          <pre className="min-w-0 flex-1 break-all whitespace-pre-wrap font-mono text-sm text-text">
            {input}
          </pre>
          <CopyButton text={input} label="Copy input" />
        </div>
      )}
      {call.output !== "" && (
        <div className="flex flex-col gap-2 border-t border-hairline pt-3">
          <div className="flex items-start gap-3">
            <pre className="max-h-64 min-w-0 flex-1 overflow-auto break-all whitespace-pre-wrap font-mono text-sm text-text tabular-nums">
              {shown}
            </pre>
            <CopyButton text={call.output} label="Copy output" />
          </div>
          {output.length > MAX_OUTPUT_LINES && (
            <p className="m-0 text-sm text-secondary tabular-nums">
              {plural(output.length - MAX_OUTPUT_LINES, "more line", "more lines")} left out
            </p>
          )}
        </div>
      )}
    </div>
  );
}

function ToolLine({
  call,
  expanded,
  onToggle,
}: {
  call: ToolCall;
  expanded: boolean;
  onToggle: () => void;
}) {
  const status = toolStatus(call);
  const check = verdict(call);
  const changes = changesOf(call);
  if (call.input.type === "todos") {
    return <ChatTodoList todos={call.input.todos} expanded={expanded} onToggle={onToggle} />;
  }
  if (changes && (call.input.type === "edit" || call.input.type === "write") && !refused(call)) {
    const { additions, deletions } = diffStats(changes);
    return (
      <ChatFileEntry
        filename={call.input.path}
        additions={additions}
        deletions={deletions}
        status={status}
        expanded={expanded}
        onToggle={onToggle}
      >
        <DiffBody changes={changes} />
      </ChatFileEntry>
    );
  }
  const hasDetails =
    call.output !== "" || (check !== null && !check.allowed) || call.input.type !== "unknown";
  return (
    <div>
      <ChatToolSummary
        summary={inputSummary(call.input)}
        detail={inputPath(call.input) ?? undefined}
        toolName={toolTitle(call)}
        status={status}
        isTruncated={hasDetails}
        expanded={expanded}
        onToggle={onToggle}
      />
      {expanded && <Details call={call} />}
    </div>
  );
}

function StepLine({
  step,
  isOpen,
  onToggle,
}: {
  step: StepView;
  isOpen: (key: string) => boolean;
  onToggle: (key: string) => void;
}) {
  if (step.kind === "thinking") {
    return (
      <ChatCollapsedThinking
        entries={[{ content: step.text, expansionKey: step.key }]}
        seconds={step.seconds}
        ongoing={step.ongoing}
        expanded={isOpen(step.key)}
        onToggle={() => onToggle(step.key)}
        renderMarkdown={({ content, className }) => (
          <Markdown content={content} className={className} keepNewlines />
        )}
      />
    );
  }
  if (step.kind === "reads") {
    return (
      <ChatAggregatedToolEntries
        label="Read"
        unit="file"
        entries={step.calls.map((call, index) => ({
          summary: inputSummary(call.input),
          detail: inputPath(call.input) ?? undefined,
          status: toolStatus(call),
          expansionKey: `${step.key}-${index}`,
        }))}
        expanded={isOpen(step.key)}
        onToggle={() => onToggle(step.key)}
      />
    );
  }
  return (
    <ToolLine call={step.call} expanded={isOpen(step.key)} onToggle={() => onToggle(step.key)} />
  );
}

/**
 * What the agent did between two texts, folded into one row: "Working" while it still goes on, "Worked · 5 steps" once
 * it has stopped. Opened, the row holds the thinking and tool rows as they were. A row of the log is 32px rather than
 * the 40px of a thing you aim at, and the rows sit 4px apart, so a log of what the agent did reads as one block of
 * small marks rather than a list to choose from. The marker is what tells `e2e/design/audit.test.ts` these rows are a
 * log at 32px rather than controls at 40px.
 */
function StepsGroup({
  block,
  isOpen,
  onToggle,
}: {
  block: Extract<Block, { kind: "steps" }>;
  isOpen: (key: string) => boolean;
  onToggle: (key: string) => void;
}) {
  const expanded = isOpen(block.key);
  return (
    <div className="flex min-w-0 flex-col">
      <button
        type="button"
        className={cn(
          "flex min-h-8 cursor-pointer items-center gap-2 self-start rounded-control px-3 text-left text-sm transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text",
          block.working ? "font-semibold text-progress-text" : "text-secondary",
        )}
        onClick={() => onToggle(block.key)}
        aria-expanded={expanded}
        data-testid={block.working ? "working" : undefined}
      >
        <CaretRightIcon
          aria-hidden="true"
          className={cn(
            "size-4 shrink-0 transition-transform duration-(--motion-routine) ease-out",
            expanded && "rotate-90",
          )}
        />
        {block.working && (
          <CircleNotchIcon aria-hidden="true" className="size-4 shrink-0 animate-spin" />
        )}
        {block.working ? (
          <span className="animate-working-shimmer bg-[linear-gradient(90deg,var(--progress-text)_35%,var(--text)_50%,var(--progress-text)_65%)] bg-[length:200%_100%] bg-clip-text text-transparent">
            Working
          </span>
        ) : (
          `Worked · ${plural(block.steps.length, "step", "steps")}`
        )}
      </button>
      {expanded && (
        <div
          className="ml-4 flex flex-col gap-1 border-l-2 border-hairline py-1 pl-3"
          data-testid="steps"
          data-audit="log"
        >
          {block.steps.map((step) => (
            <StepLine key={step.key} step={step} isOpen={isOpen} onToggle={onToggle} />
          ))}
        </div>
      )}
    </div>
  );
}

function BlockView({
  block,
  isOpen,
  onToggle,
  retry,
  onAnswer,
  sending,
}: {
  block: Block;
  isOpen: (key: string) => boolean;
  onToggle: (key: string) => void;
  /** What to press to have the failed turn answered again; only the last error of the turn has one. */
  retry?: ReactNode;
  /** Sends the answer to the question this block asks, when it is the one the chat waits on. */
  onAnswer?: (message: string) => void;
  sending?: boolean;
}) {
  switch (block.kind) {
    case "text":
      return (
        <ChatAssistantMessage
          content={block.text}
          renderMarkdown={({ content }) => (
            // The answer keeps the measure of the standard and takes every line the agent wrote; the wrapper the
            // component copied is one box that says the same.
            <div className="max-w-prose">
              <Markdown content={content} className="break-words" keepNewlines />
            </div>
          )}
          actions={<CopyButton text={block.text} label="Copy message" />}
        />
      );
    case "error":
      return (
        <ChatErrorMessage
          title={block.source === "model" ? "The model returned an error" : "The run failed"}
          content={block.message}
          // The way back sits beside Copy: taking the words away is still worth doing when going again.
          actions={
            retry ? (
              <div className="flex items-center gap-2">
                {retry}
                <CopyButton text={block.message} label="Copy error" />
              </div>
            ) : (
              <CopyButton text={block.message} label="Copy error" />
            )
          }
        />
      );
    case "mcpError":
      return (
        <ChatErrorMessage
          title={`MCP server ${block.server} could not be reached`}
          content={block.message}
          actions={<CopyButton text={block.message} label="Copy error" />}
        />
      );
    case "question":
      return (
        <ChatQuestion
          questions={block.questions}
          answer={block.answer}
          onAnswer={onAnswer}
          sending={sending}
        />
      );
    case "steps":
      return <StepsGroup block={block} isOpen={isOpen} onToggle={onToggle} />;
  }
}

interface TurnProps {
  turn: TurnData;
  /** The keys of the steps that are unfolded. */
  unfolded: ReadonlySet<string>;
  onToggle: (key: string) => void;
  /** A tighter rhythm between the blocks and the rows, for the floating chat's dock. */
  compact?: boolean;
  /** Offered on the last block of the turn that went wrong: what to press to have it answered again. */
  retry?: ReactNode;
  /**
   * Sends the person's answer to the agent's question. Given to the last turn only, and taken by the last question of
   * it: the agent often writes a line after its question ("waiting on your answers"), which does not answer it.
   */
  onAnswer?: (message: string) => void;
  /** Whether a message is on its way, so an answer is not sent twice. */
  sending?: boolean;
}

export function Turn({
  turn,
  unfolded,
  onToggle,
  compact = false,
  retry,
  onAnswer,
  sending,
}: TurnProps) {
  const isOpen = (key: string) => unfolded.has(key);
  const head = turn.head;
  // A turn is written on the canvas rather than stacked in boxes: the person's message is a band of the sunken
  // colour, and what the agent did, answered and got wrong is the text and the rows under it, in the order it came. A
  // card of its own would put a second shadow inside the chat, which is itself a card on the page or a sheet over it.
  // Going again answers the whole turn once, so only its last error offers it.
  const lastError = turn.blocks.findLast((block) => block.kind === "error")?.key;
  const lastQuestion = turn.blocks.findLast((block) => block.kind === "question")?.key;
  const view = (block: Block) => (
    <BlockView
      key={block.key}
      block={block}
      isOpen={isOpen}
      onToggle={onToggle}
      retry={block.key === lastError ? retry : undefined}
      onAnswer={block.key === lastQuestion ? onAnswer : undefined}
      sending={sending}
    />
  );
  return (
    <div data-testid="turn" className={cn("flex min-w-0 flex-col", compact ? "gap-3" : "gap-4")}>
      {head && (
        <ChatUserMessage
          content={head.message}
          title={head.said === "steered" ? "You steered the chat" : "You"}
          className="rounded-card"
          headerRight={<CopyButton text={head.message} label="Copy your message" />}
          renderMarkdown={({ content }) => (
            <p
              className="max-w-prose break-words whitespace-pre-wrap text-base leading-relaxed text-text"
              data-testid="person-message"
            >
              {content}
            </p>
          )}
        />
      )}
      {turn.blocks.map(view)}
    </div>
  );
}
