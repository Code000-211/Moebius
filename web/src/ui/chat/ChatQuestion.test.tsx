// @vitest-environment jsdom
/**
 * The agent's questions: parsed from the question tool of either harness, set apart from the steps, and answered as the
 * person's message.
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { type Entry, toolInput } from "../../chat/conversation";
import { groupTurns } from "../../chat/turns";
import { ChatQuestion } from "./ChatQuestion";
import { Turn } from "./Turn";

afterEach(cleanup);

const args = {
  questions: [
    {
      header: "Database",
      question: "Which database?",
      multiSelect: false,
      options: [
        { label: "SQLite", description: "One file" },
        { label: "Postgres", description: "A server" },
      ],
    },
    {
      header: "Extras",
      question: "Which extras?",
      multiSelect: true,
      options: [{ label: "Metrics" }, { label: "Tracing" }],
    },
  ],
};

describe("toolInput", () => {
  it("reads the questions of Claude Code's tool and Pi's", () => {
    for (const tool of ["AskUserQuestion", "ask_user_question"]) {
      const input = toolInput(tool, args);
      expect(input.type).toBe("questions");
      if (input.type !== "questions") continue;
      expect(input.questions.map((q) => [q.header, q.multiple, q.options.length])).toEqual([
        ["Database", false, 2],
        ["Extras", true, 2],
      ]);
    }
  });

  it("keeps a call without a question an ordinary tool call", () => {
    expect(toolInput("AskUserQuestion", { questions: [] }).type).toBe("other");
  });
});

describe("a turn that ends in a question", () => {
  const entries: Entry[] = [
    { at: 1, seq: 1, kind: "person", said: "said", message: "Build it" },
    { at: 2, seq: 2, kind: "thinking", text: "hm" },
    {
      at: 3,
      seq: 3,
      kind: "tool",
      call: {
        id: "q",
        tool: "ask_user_question",
        input: toolInput("ask_user_question", args),
        check: null,
        mcp: null,
        execution: null,
        output: "",
      },
    },
  ];
  const turn = () => {
    const first = groupTurns(entries, false)[0];
    if (!first) throw new Error("one turn");
    return first;
  };

  it("sets the question apart from the steps before it", () => {
    expect(turn().blocks.map((block) => block.kind)).toEqual(["steps", "question"]);
  });

  it("sends the chosen options, one line for each question", () => {
    const onAnswer = vi.fn();
    render(<Turn turn={turn()} unfolded={new Set()} onToggle={() => {}} onAnswer={onAnswer} />);
    const send = screen.getByRole("button", { name: "Send answer" });
    expect(send.hasAttribute("disabled")).toBe(true);
    fireEvent.click(screen.getByLabelText(/Postgres/));
    fireEvent.click(screen.getByLabelText("Metrics"));
    fireEvent.click(screen.getByLabelText("Tracing"));
    fireEvent.click(send);
    expect(onAnswer).toHaveBeenCalledWith("Database: Postgres\nExtras: Metrics, Tracing");
  });

  it("is still answered when the agent wrote a line after it", () => {
    const after: Entry[] = [
      ...entries,
      { at: 4, seq: 4, kind: "text", text: "Waiting on your answers." },
    ];
    const first = groupTurns(after, false)[0];
    if (!first) throw new Error("one turn");
    render(<Turn turn={first} unfolded={new Set()} onToggle={() => {}} onAnswer={() => {}} />);
    expect(screen.getByRole("button", { name: "Send answer" })).toBeTruthy();
  });

  it("shows what the person chose once their message followed it", () => {
    const followed: Entry[] = [
      ...entries,
      {
        at: 4,
        seq: 4,
        kind: "person",
        said: "said",
        message: "Database: Postgres\nExtras: Metrics, my own",
      },
    ];
    const first = groupTurns(followed, false)[0];
    if (!first) throw new Error("one turn");
    render(<Turn turn={first} unfolded={new Set()} onToggle={() => {}} />);
    const closed = screen.getByTestId("question-asked");
    const chosen = Array.from(closed.querySelectorAll("li"))
      .filter((li) => li.textContent?.includes("chosen") || li.textContent?.startsWith("Chosen"))
      .map((li) => li.textContent);
    expect(chosen).toEqual(["Postgres (chosen)", "Metrics (chosen)", "Chosen, Other: my own"]);
  });

  it("marks the recommended option and gives its answer without the mark", () => {
    const input = toolInput("ask_user_question", {
      questions: [
        {
          question: "Which?",
          header: "H",
          options: [
            { label: "A (Recommended)", description: "" },
            { label: "B", description: "" },
          ],
        },
      ],
    });
    if (input.type !== "questions") throw new Error("questions");
    const onAnswer = vi.fn();
    render(<ChatQuestion questions={input.questions} onAnswer={onAnswer} />);
    expect(screen.getByText("Recommended")).toBeTruthy();
    fireEvent.click(screen.getByLabelText(/^A/));
    fireEvent.click(screen.getByRole("button", { name: "Send answer" }));
    expect(onAnswer).toHaveBeenCalledWith("A");
  });

  it("is a record once the conversation went past it", () => {
    render(<Turn turn={turn()} unfolded={new Set()} onToggle={() => {}} />);
    expect(screen.queryByRole("button", { name: "Send answer" })).toBeNull();
    expect(screen.getByTestId("question-asked").textContent).toContain("Which database?");
  });
});

describe("ChatQuestion", () => {
  const [only] = toolInput("AskUserQuestion", args).type === "questions" ? [args] : [];
  it("sends the words of Other alone for a single question", () => {
    const onAnswer = vi.fn();
    const input = toolInput("AskUserQuestion", { questions: only?.questions.slice(0, 1) });
    if (input.type !== "questions") throw new Error("questions");
    render(<ChatQuestion questions={input.questions} onAnswer={onAnswer} />);
    fireEvent.click(screen.getByLabelText("Other"));
    const field = screen.getByLabelText("Your answer to: Which database?");
    // Chosen, but empty: nothing to send yet.
    expect(screen.getByRole("button", { name: "Send answer" }).hasAttribute("disabled")).toBe(true);
    fireEvent.change(field, { target: { value: "DuckDB" } });
    fireEvent.click(screen.getByRole("button", { name: "Send answer" }));
    expect(onAnswer).toHaveBeenCalledWith("DuckDB");
  });
});
