// @vitest-environment jsdom
/**
 * The agent's questions: parsed from the question tool of either harness, set apart from the steps, and answered as the
 * person's message.
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Chosen } from "../../chat/answer";
import { answerMessage, answersIn } from "../../chat/answer";
import { type AskedQuestion, type Entry, toolInput } from "../../chat/conversation";
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
    expect(onAnswer).toHaveBeenCalledWith(
      'My answers:\n1. Database: "Postgres"\n2. Extras: "Metrics", "Tracing"',
    );
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
        message: 'My answers:\n1. Database: "Postgres"\n2. Extras: "Metrics", Other: "my own"',
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
    expect(onAnswer).toHaveBeenCalledWith('My answers:\n1. H: "A"');
  });

  it("is a record once the conversation went past it", () => {
    render(<Turn turn={turn()} unfolded={new Set()} onToggle={() => {}} />);
    expect(screen.queryByRole("button", { name: "Send answer" })).toBeNull();
    expect(screen.getByTestId("question-asked").textContent).toContain("Which database?");
  });
});

describe("ChatQuestion", () => {
  const [only] = toolInput("AskUserQuestion", args).type === "questions" ? [args] : [];
  it("sends the words of Other for a single question", () => {
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
    expect(onAnswer).toHaveBeenCalledWith('My answers:\n1. Database: Other: "DuckDB"');
  });
});

describe("the answer message", () => {
  const asked = (questions: unknown[]): AskedQuestion[] => {
    const input = toolInput("ask_user_question", { questions });
    if (input.type !== "questions") throw new Error("questions");
    return input.questions;
  };
  const yes = asked([
    {
      header: "Go",
      question: "Go on?",
      options: [{ label: "Yes, but later" }, { label: "Yes" }, { label: "No" }],
    },
  ]);
  const picks = (chosen: Chosen | null) => (chosen ? [...chosen.picked] : null);

  it("marks the option of the label that was written, not another that holds it", () => {
    const [yesOnly] = answersIn(yes, 'My answers:\n1. Go: "Yes"');
    expect(picks(yesOnly ?? null)).toEqual([1]);
    const [later] = answersIn(yes, 'My answers:\n1. Go: "Yes, but later"');
    expect(picks(later ?? null)).toEqual([0]);
  });

  it("takes words that hold a label for the person's own words, which mark nothing", () => {
    const [own] = answersIn(yes, 'My answers:\n1. Go: Other: "No, not yes"');
    expect(picks(own ?? null)).toEqual([]);
    expect(own?.own).toBe("No, not yes");
  });

  it("keeps two questions of one header apart, and a label with a comma whole", () => {
    const two = asked([
      { header: "Pick", question: "First?", options: [{ label: "A, B" }, { label: "A" }] },
      { header: "Pick", question: "Second?", options: [{ label: "A" }, { label: "B" }] },
    ]);
    const message = answerMessage(two, [
      { picked: [0], other: false, words: "" },
      { picked: [1], other: false, words: "" },
    ]);
    expect(message).toBe('My answers:\n1. Pick: "A, B"\n2. Pick: "B"');
    const [first, second] = answersIn(two, message);
    expect(picks(first ?? null)).toEqual([0]);
    expect(picks(second ?? null)).toEqual([1]);
  });

  it("marks nothing for a message that is not in the shape, however much it resembles one", () => {
    for (const said of [
      "Go: Yes",
      'My answers:\n1. Go: "Maybe"',
      'My answers:\n1. Go: "Yes" and more',
      'My answers:\n2. Go: "Yes"',
      'My answers:\n1. Go: "Yes"\nlater',
      "Yes",
    ]) {
      expect(answersIn(yes, said), said).toEqual([null]);
    }
    expect(answersIn(yes, null)).toEqual([null]);
  });

  it("round-trips quotes, newlines and Other together with options", () => {
    const [question] = yes;
    if (!question) throw new Error("one");
    const message = answerMessage(yes, [
      { picked: [2, 1], other: true, words: 'say "hi"\nthen go' },
    ]);
    const [chosen] = answersIn(yes, message);
    expect(picks(chosen ?? null)).toEqual([1, 2]);
    expect(chosen?.own).toBe('say "hi"\nthen go');
  });

  it("cuts a header and a label past their limits, and keeps the whole as the tooltip", () => {
    const long = "L".repeat(70);
    const questions = asked([
      {
        header: "H".repeat(20),
        question: "Which?",
        options: [{ label: long }, { label: "short" }],
      },
    ]);
    render(<ChatQuestion questions={questions} onAnswer={() => {}} />);
    expect(screen.getByTitle("H".repeat(20)).textContent).toBe(`${"H".repeat(15)}…`);
    expect(screen.getByTitle(long).textContent).toBe(`${"L".repeat(59)}…`);
  });

  it("holds two questions and two options alike without losing the choice of either", () => {
    const same = asked([
      { header: "Same", question: "Same?", options: [{ label: "X" }, { label: "X" }] },
      { header: "Same", question: "Same?", options: [{ label: "X" }, { label: "Y" }] },
    ]);
    render(<ChatQuestion questions={same} onAnswer={() => {}} />);
    const boxes = screen.getAllByLabelText("X") as HTMLInputElement[];
    expect(boxes).toHaveLength(3);
    fireEvent.click(boxes[1] as HTMLInputElement);
    fireEvent.click(boxes[2] as HTMLInputElement);
    const checked = boxes.map((box) => box.checked);
    expect(checked).toEqual([false, true, true]);
  });
});
