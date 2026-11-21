/**
 * What a tool call is, read from its arguments and shown as a line: its input (`toolInput`, for Pi's lower case tool
 * names and Claude Code's capitalised ones), the questions of the question tool, the one-line summary, the path, the
 * decision of the check and the name the call is shown under. Part of the conversation of `Conversation.ts`.
 */
import { fileName } from "../../ui/lib/format";
import { type Json, object, string } from "./json";
import type { AskedQuestion, Check, ToolCall, ToolInput } from "./types";

const RECOMMENDED = /\s*\(recommended\)\s*$/i;

/**
 * The questions of a call to the question tool (Claude Code's `AskUserQuestion`, Pi's `ask_user_question`), or null when
 * the arguments hold none. Both take `questions`, each with a `question`, a `header`, `options` of a `label` and a
 * `description` (a bare string is a label) and `multiSelect`.
 */
function askedQuestions(a: Json): AskedQuestion[] | null {
  const list = Array.isArray(a.questions) ? a.questions : typeof a.question === "string" ? [a] : [];
  const questions = list.flatMap((each): AskedQuestion[] => {
    const q = object(each);
    const question = string(q.question);
    if (question === "") return [];
    const options = (Array.isArray(q.options) ? q.options : []).flatMap((option) => {
      const written = typeof option === "string" ? option : string(object(option).label);
      const label = written.replace(RECOMMENDED, "").trim();
      return label === ""
        ? []
        : [
            {
              label,
              description: string(object(option).description),
              recommended: label !== written.trim(),
            },
          ];
    });
    return [
      {
        header: string(q.header),
        question,
        multiple: q.multiSelect === true || q.multiple === true,
        options,
      },
    ];
  });
  return questions.length > 0 ? questions : null;
}

/** Whether `tool` is the question tool, in either harness's spelling. */
export function asksQuestions(tool: string): boolean {
  return tool.toLowerCase().replace(/[_-]/g, "") === "askuserquestion";
}

/** The input of a tool call from its arguments; Pi's tool names are lower case, Claude Code's are capitalised. */
export function toolInput(tool: string, args: unknown): ToolInput {
  const a = object(args);
  const path = string(a.path) || string(a.file_path);
  if (asksQuestions(tool)) {
    const questions = askedQuestions(a);
    if (questions) return { type: "questions", questions };
  }
  switch (tool.toLowerCase()) {
    case "read":
      return { type: "read", path };
    case "write":
      return { type: "write", path, content: string(a.content) };
    case "bash":
      return { type: "command", command: string(a.command) };
    case "edit": {
      // Pi's edit takes `edits`, and still the single `oldText` and `newText` of its older form; Claude Code's takes
      // `old_string` and `new_string`.
      const changes = Array.isArray(a.edits)
        ? a.edits.map((edit) => ({
            oldText: string(object(edit).oldText),
            newText: string(object(edit).newText),
          }))
        : [];
      if (typeof a.oldText === "string") {
        changes.push({ oldText: a.oldText, newText: string(a.newText) });
      }
      if (typeof a.old_string === "string") {
        changes.push({ oldText: a.old_string, newText: string(a.new_string) });
      }
      return { type: "edit", path, changes };
    }
    case "todowrite": {
      if (!Array.isArray(a.todos)) return { type: "other", json: JSON.stringify(a) };
      return {
        type: "todos",
        todos: a.todos.map((todo) => ({
          content: string(object(todo).content),
          status: string(object(todo).status),
        })),
      };
    }
    default:
      return args === null || args === undefined
        ? { type: "unknown" }
        : { type: "other", json: JSON.stringify(args) };
  }
}

/** The input in one line, as the pill of a tool call shows it. */
export function inputSummary(input: ToolInput): string {
  switch (input.type) {
    case "unknown":
      return "";
    case "read":
    case "edit":
    case "write":
      return fileName(input.path);
    case "command":
      return input.command.split("\n")[0] ?? "";
    case "todos":
      return `${input.todos.length} items`;
    case "questions":
      return input.questions[0]?.question ?? "";
    case "other":
      return input.json;
  }
}

/**
 * The whole path a file call names, for the tooltip of a row that shows only the file's name; null for the calls
 * that are not about one file.
 */
export function inputPath(input: ToolInput): string | null {
  switch (input.type) {
    case "read":
    case "edit":
    case "write":
      return input.path;
    default:
      return null;
  }
}

/** Which small icon a step of the thread carries. */
export type StepIcon = "read" | "edit" | "terminal" | "search" | "test" | "check" | "web" | "tool";

/** What a step says of itself: a few plain words, and the name of the file when it is about one (set in mono). */
export interface StepWords {
  icon: StepIcon;
  text: string;
  code?: string;
}

/** The command that is run, without a leading `cd dir &&` or environment assignments, in lower case. */
function bareCommand(command: string): string {
  const line = (command.split("\n")[0] ?? "").trim();
  const last = line.split("&&").pop() ?? line;
  return last
    .replace(/^(\s*[A-Z_][A-Z0-9_]*=\S*\s+)+/, "")
    .trim()
    .toLowerCase();
}

/** What a command does, in words; `[done, going]` is how it reads once it ended and while it runs. */
function commandWords(command: string): [StepIcon, string, string] {
  const bare = bareCommand(command);
  const has = (pattern: RegExp) => pattern.test(bare);
  if (has(/\b(test|pytest|vitest|jest|nextest)\b/))
    return ["test", "Ran the tests", "Running the tests"];
  if (has(/^(rg|grep|ag|fd|find)\b/)) return ["search", "Searched the code", "Searching the code"];
  if (has(/^(ls|tree)\b/)) return ["search", "Listed the files", "Listing the files"];
  if (has(/\b(clippy|fmt|lint|check|tsc|biome|eslint|prettier)\b/))
    return ["check", "Ran the checks", "Running the checks"];
  if (has(/\b(build|compile)\b/)) return ["terminal", "Built the project", "Building the project"];
  if (has(/^git\s+(diff|status|log|show)\b/))
    return ["terminal", "Looked at the changes", "Looking at the changes"];
  if (has(/^git\s+commit\b/))
    return ["terminal", "Committed the changes", "Committing the changes"];
  if (has(/^git\s+push\b/)) return ["terminal", "Pushed the branch", "Pushing the branch"];
  if (has(/^git\b/)) return ["terminal", "Used git", "Using git"];
  if (has(/moebius"?\s+board\b/)) return ["tool", "Looked at the board", "Looking at the board"];
  if (has(/moebius"?\s+card\b/)) return ["tool", "Read a card", "Reading a card"];
  if (has(/moebius"?\s/)) return ["tool", "Asked Moebius", "Asking Moebius"];
  if (has(/^(psql|sqlite3|mysql)\b/))
    return ["terminal", "Queried the database", "Querying the database"];
  if (has(/^(curl|wget)\b/)) return ["web", "Fetched a page", "Fetching a page"];
  return ["terminal", "Ran a command", "Running a command"];
}

/**
 * What a tool call did, as the thread says it: "Read wiki.rs", "Ran the tests", "Searched the code". The raw command
 * or the arguments are not here; the step shows them once it is opened.
 */
export function stepWords(call: ToolCall): StepWords {
  const running = call.execution?.state !== "finished";
  const { input } = call;
  const file = inputPath(input) ?? undefined;
  const named = file ? fileName(file) : undefined;
  switch (input.type) {
    case "read":
      return { icon: "read", text: running ? "Reading" : "Read", code: named };
    case "edit":
      return { icon: "edit", text: running ? "Editing" : "Edited", code: named };
    case "write":
      return { icon: "edit", text: running ? "Writing" : "Wrote", code: named };
    case "command": {
      const [icon, done, going] = commandWords(input.command);
      return { icon, text: running ? going : done };
    }
    default: {
      const tool = call.tool.toLowerCase();
      if (/^(grep|find|glob|search)/.test(tool))
        return { icon: "search", text: running ? "Searching the code" : "Searched the code" };
      if (tool === "ls")
        return { icon: "search", text: running ? "Listing the files" : "Listed the files" };
      if (/^web/.test(tool))
        return {
          icon: "web",
          text: running ? "Looking it up on the web" : "Looked it up on the web",
        };
      return { icon: "tool", text: `${running ? "Using" : "Used"} ${toolTitle(call)}` };
    }
  }
}

/** The decision the call is shown with: a refusal of the check or the gateway, else the check's answer, else the gateway's. */
export function verdict(call: ToolCall): Check | null {
  const checks = [call.check, call.mcp?.check ?? null].filter((check) => check !== null);
  return checks.find((check) => !check.allowed) ?? checks[0] ?? null;
}

export function refused(call: ToolCall): boolean {
  return verdict(call)?.allowed === false;
}

/** The name the call is shown under: `<server> · <tool>` for a call of an MCP server, else the tool's. */
export function toolTitle(call: ToolCall): string {
  return call.mcp ? `${call.mcp.server} · ${call.mcp.tool}` : call.tool;
}

/** The text of a tool's result: the text blocks of its content, joined. */
export function resultText(result: unknown): string {
  const content = object(result).content;
  if (!Array.isArray(content)) return "";
  return content
    .map((block) => object(block).text)
    .filter((text): text is string => typeof text === "string")
    .join("\n");
}
