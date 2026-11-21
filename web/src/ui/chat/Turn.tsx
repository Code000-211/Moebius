/**
 * One turn of a conversation (the former desktop app's CH3-D): the person's message as its head and what the agent did and
 * answered as its body. Presentational: the turn comes in as data (see `chat/turns.ts`), and so do the folded state
 * and the clicks. A later page that shows a run's conversation draws it with this.
 *
 * The person's message is a bubble on the right; the answer is free text across the width of the column. What the agent
 * did is one line apiece — thinking, a tool, a group of reads, a changed file, a todo list — each 32px tall with an
 * icon, a label and its duration or state, opening to the text it worked with on the colour of code. A long run of
 * steps is folded into one row.
 */
import { CaretRightIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import {
  inputPath,
  inputSummary,
  refused,
  stepWords,
  type ToolCall,
  toolTitle,
  verdict,
} from "../../chat/conversation";
import { useThinkingExpanded } from "../../chat/thinking";
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

/** From this many steps on, a block of steps is folded into one row; fewer are drawn as the timeline. */
const FOLD_FROM = 5;

/**
 * The steps of a turn hang on one thin line: it runs down the middle of their icons (16px wide, so 8px in), which sit on
 * the page's colour and break it where they are. The rows sit 6px apart, as the standard asks of a body of small marks.
 */
const STEP_LINE =
  "relative flex min-w-0 flex-col gap-1.5 before:absolute before:top-3 before:bottom-3 before:left-[7.5px] before:w-px before:bg-hairline-strong";

/** The status a tool call is in, told by one of the words `ToolStatusDot` knows. */
export function toolStatus(call: ToolCall): ToolStatusLike {
  if (refused(call)) return { status: "denied" };
  if (call.execution?.state === "finished") {
    return { status: call.execution.failed ? "failed" : "success" };
  }
  return { status: "created" };
}

/** The status a step row says: a call that went well says nothing, one that did not says what happened. */
function quiet(status: ToolStatusLike): ToolStatusLike | undefined {
  return status.status === "success" ? undefined : status;
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
        status={quiet(status)}
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
        words={stepWords(call)}
        detail={inputPath(call.input) ?? toolTitle(call)}
        status={quiet(status)}
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
  shownOpen,
}: {
  step: StepView;
  isOpen: (key: string) => boolean;
  onToggle: (key: string) => void;
  /** Whether the person's setting has finished thinking open. */
  shownOpen: boolean;
}) {
  if (step.kind === "thinking") {
    // A thought that is still arriving is open and cannot be closed; one that has stopped follows the person's setting
    // until they open or close it themselves, which is kept under the key of the step with `:closed` after it when the
    // setting has it open, so a choice made against the setting is not overwritten by it.
    const closedKey = `${step.key}:closed`;
    const expanded = step.ongoing || (shownOpen ? !isOpen(closedKey) : isOpen(step.key));
    return (
      <ChatCollapsedThinking
        entries={[{ content: step.text, expansionKey: step.key }]}
        seconds={step.seconds}
        ongoing={step.ongoing}
        expanded={expanded}
        onToggle={() => {
          if (!step.ongoing) onToggle(shownOpen ? closedKey : step.key);
        }}
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
          status: quiet(toolStatus(call)),
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

/** The tool calls of a block of steps, with the runs of reads taken apart again. */
function callsOf(steps: StepView[]): ToolCall[] {
  return steps.flatMap((step) =>
    step.kind === "tool" ? [step.call] : step.kind === "reads" ? step.calls : [],
  );
}

/** How many calls of a block of steps the check refused, and how many ran and failed. */
function troubleOf(steps: StepView[]): { refused: number; failed: number } {
  let refusedCount = 0;
  let failedCount = 0;
  for (const call of callsOf(steps)) {
    const { status } = toolStatus(call);
    if (status === "denied") refusedCount += 1;
    else if (status === "failed") failedCount += 1;
  }
  return { refused: refusedCount, failed: failedCount };
}

/**
 * What the agent did between two texts, folded into one row: "Working" while it still goes on, "Worked · 5 steps" once
 * it has stopped. Opened, the row holds the thinking and tool rows as they were. A step that was refused or failed is
 * said on the row itself, in the colour of an error, and the row opens by itself then: what went wrong is not
 * something to fold away. The person can still close it, which is kept under the key of the block with `:closed`
 * after it, so a group that opens itself and one the person opened do not read the same set the same way. A row of the
 * log is 32px, a row of a list in the quiet standard, and the rows sit 4px apart, so a log of what the agent did
 * reads as one block of small marks rather than a list to choose from.
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
  const shownOpen = useThinkingExpanded();
  const trouble = troubleOf(block.steps);
  const troubled = trouble.refused + trouble.failed > 0;
  const rows = block.steps.map((step) => (
    <StepLine
      key={step.key}
      step={step}
      isOpen={isOpen}
      onToggle={onToggle}
      shownOpen={shownOpen}
    />
  ));
  // The group opens by itself for trouble, for a thought still arriving (it would be hidden in a folded group) and
  // when the person's setting has thinking open; a choice of theirs is kept under `:closed` as it is for trouble.
  const opensItself =
    troubled || shownOpen || block.steps.some((step) => step.kind === "thinking" && step.ongoing);
  // A few steps are the timeline itself, one line each; only a long run of them, or one with trouble in it, is folded.
  if (!troubled && block.steps.length < FOLD_FROM) {
    return (
      <div className={STEP_LINE} data-testid="steps">
        {rows}
        {block.working && (
          <p
            className="m-0 flex min-h-5 items-center pl-[26px] text-sm text-progress-text"
            data-testid="working"
          >
            Working
          </p>
        )}
      </div>
    );
  }
  const closedKey = `${block.key}:closed`;
  const expanded = opensItself ? !isOpen(closedKey) : isOpen(block.key);
  const bodyId = `${block.key}-steps`;
  const said = [
    trouble.refused > 0 ? `${trouble.refused} refused` : null,
    trouble.failed > 0 ? `${trouble.failed} failed` : null,
  ].filter((part) => part !== null);
  return (
    <div className="flex min-w-0 flex-col">
      <button
        type="button"
        className={cn(
          "flex min-h-5 cursor-pointer items-center gap-2 self-start text-left text-sm transition-colors duration-(--motion-routine) ease-out hover:text-secondary",
          block.working ? "text-progress-text" : "text-muted",
        )}
        onClick={() => onToggle(opensItself ? closedKey : block.key)}
        aria-expanded={expanded}
        aria-controls={expanded ? bodyId : undefined}
        data-testid={block.working ? "working" : undefined}
      >
        <CaretRightIcon
          aria-hidden="true"
          className={cn(
            "size-4 shrink-0 transition-transform duration-(--motion-routine) ease-out",
            expanded && "rotate-90",
          )}
        />
        {block.working ? "Working" : `Worked · ${plural(block.steps.length, "step", "steps")}`}
        {said.length > 0 && (
          <span className="font-semibold text-error-text" data-testid="steps-trouble">
            {` · ${said.join(" · ")}`}
          </span>
        )}
      </button>
      {expanded && (
        <div id={bodyId} className={cn(STEP_LINE, "mt-1.5")} data-testid="steps">
          {rows}
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
            // The answer takes the width of the column; a cap of its own would leave the room the conversation has empty.
            <Markdown content={content} className="break-words" keepNewlines />
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
  /** A tighter rhythm between the blocks and the rows, for the Moebius agent's sheet. */
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
  // A turn is written on the canvas rather than stacked in boxes: the person's message is a bubble on the right, and
  // what the agent did, answered and got wrong is the text and the rows under it, in the order it came.
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
          steered={head.said === "steered"}
          copy={<CopyButton text={head.message} label="Copy your message" />}
          renderMarkdown={({ content }) => (
            <p
              className="m-0 break-words whitespace-pre-wrap text-base leading-relaxed text-text"
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
