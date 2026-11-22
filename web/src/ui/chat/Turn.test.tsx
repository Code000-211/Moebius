// @vitest-environment jsdom
/**
 * The block that says what went wrong holds the way back, when the page has one to offer: the chat whose agent went
 * away is answered again from the words it was answering, and the failure is where the person is already looking.
 */
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import type { Entry, ToolCall } from "../../chat/conversation";
import { ThinkingProvider } from "../../chat/thinking";
import { groupTurns, type Turn as TurnData } from "../../chat/turns";
import { Turn } from "./Turn";

afterEach(cleanup);

/** The turn a message that was never answered makes: the question, then why it was not answered. */
function failedTurn(): Entry[] {
  return [
    { at: 1, seq: 1, kind: "person", said: "said", message: "Broken" },
    { at: 2, seq: 2, kind: "error", source: "run", message: "the server stopped" },
  ];
}

function turnOf(entries: Entry[]) {
  const turn = groupTurns(entries, false)[0];
  if (!turn) throw new Error("these entries group into one turn");
  return turn;
}

const folding = { unfolded: new Set<string>(), onToggle: () => {} };

/** A turn whose steps open when clicked, as the page keeps them. */
function Open({ turn }: { turn: TurnData }) {
  const [unfolded, setUnfolded] = useState(new Set<string>());
  return (
    <Turn
      turn={turn}
      unfolded={unfolded}
      onToggle={(key) => setUnfolded((all) => new Set(all).add(key))}
    />
  );
}

describe("a turn that ended in a failure", () => {
  it("puts the way back beside the words that say what went wrong", () => {
    render(
      <Turn
        turn={turnOf(failedTurn())}
        {...folding}
        retry={<button type="button">Retry</button>}
      />,
    );
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("the server stopped");
    expect(within(alert).getByRole("button", { name: "Retry" })).toBeTruthy();
    // Copy stays beside the way back.
    expect(within(alert).getByRole("button", { name: "Copy error" })).toBeTruthy();
  });

  it("gives the way back to the last error of the turn only", () => {
    const twice: Entry[] = [
      ...failedTurn(),
      { at: 3, seq: 3, kind: "error", source: "model", message: "the model said no" },
    ];
    render(<Turn turn={turnOf(twice)} {...folding} retry={<button type="button">Retry</button>} />);
    const alerts = screen.getAllByRole("alert");
    expect(alerts).toHaveLength(2);
    expect(within(alerts[0] as HTMLElement).queryByRole("button", { name: "Retry" })).toBeNull();
    expect(within(alerts[1] as HTMLElement).getByRole("button", { name: "Retry" })).toBeTruthy();
    for (const alert of alerts) {
      expect(within(alert).getByRole("button", { name: "Copy error" })).toBeTruthy();
    }
  });

  it("says only what went wrong where there is nothing to press", () => {
    render(<Turn turn={turnOf(failedTurn())} {...folding} />);
    const alert = screen.getByRole("alert");
    expect(within(alert).queryByRole("button", { name: "Retry" })).toBeNull();
    expect(within(alert).getByRole("button", { name: "Copy error" })).toBeTruthy();
  });

  it("offers nothing on a turn that was answered", () => {
    const answered: Entry[] = [
      { at: 1, seq: 1, kind: "person", said: "said", message: "Broken" },
      { at: 2, seq: 2, kind: "text", text: "Here is what happened." },
    ];
    render(
      <Turn turn={turnOf(answered)} {...folding} retry={<button type="button">Retry</button>} />,
    );
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryAllByRole("button", { name: "Retry" })).toHaveLength(0);
  });
});

describe("what the agent did", () => {
  const worked: Entry[] = [
    { at: 1, seq: 1, kind: "person", said: "said", message: "Do it" },
    { at: 2, seq: 2, kind: "thinking", text: "Let me look." },
    {
      at: 4,
      seq: 3,
      kind: "tool",
      call: {
        id: "t1",
        tool: "bash",
        input: { type: "command", command: "ls" },
        output: "",
        check: null,
        mcp: null,
        execution: { state: "finished", failed: false },
      },
    },
  ];

  it("draws a few steps as the timeline itself, one line each, with nothing to open", () => {
    render(<Turn turn={turnOf(worked)} {...folding} />);
    expect(screen.queryByRole("button", { name: /^Worked/ })).toBeNull();
    const steps = screen.getByTestId("steps");
    expect(steps.textContent).toContain("Thought");
    expect(steps.textContent).toContain("Listed the files");
    expect(steps.textContent).not.toContain("ls");
  });

  describe("a long run of steps", () => {
    const many: Entry[] = [
      { at: 1, seq: 1, kind: "person", said: "said", message: "Do it" },
      ...[1, 2, 3, 4, 5].map(
        (n): Entry => ({
          at: 1 + n,
          seq: 1 + n,
          kind: "tool",
          call: {
            id: `m${n}`,
            tool: "bash",
            input: { type: "command", command: `ls ${n}` },
            output: "",
            check: null,
            mcp: null,
            execution: { state: "finished", failed: false },
          },
        }),
      ),
    ];

    it("is one folded row that counts the steps, and shows none of them", () => {
      render(<Turn turn={turnOf(many)} {...folding} />);
      const row = screen.getByRole("button", { name: "Worked · 5 steps" });
      expect(row.getAttribute("aria-expanded")).toBe("false");
      expect(screen.queryByTestId("steps")).toBeNull();
    });

    it("holds the steps once it is unfolded", () => {
      const turn = turnOf(many);
      const key = turn.blocks[0]?.key as string;
      render(<Turn turn={turn} unfolded={new Set([key])} onToggle={() => {}} />);
      expect(screen.getByTestId("steps").textContent).toContain("Listed the files");
    });
  });

  it("puts the person's message in a bubble on the right", () => {
    render(<Turn turn={turnOf(worked)} {...folding} />);
    const bubble = screen.getByTestId("person-message").parentElement?.parentElement;
    expect(bubble?.className).toContain("items-center");
    expect(bubble?.parentElement?.className).toContain("items-end");
  });

  it("draws nothing that moves or is masked while the agent works", () => {
    const turn = groupTurns(worked, true)[0];
    if (!turn) throw new Error("one turn");
    const { container } = render(<Turn turn={turn} {...folding} />);
    for (const element of container.querySelectorAll("*")) {
      const classes = element.getAttribute("class") ?? "";
      expect(classes).not.toMatch(/animate-|shimmer|mask/);
    }
  });

  it("says Working while the agent is at it", () => {
    const turn = groupTurns(worked, true)[0];
    if (!turn) throw new Error("one turn");
    render(<Turn turn={turn} {...folding} />);
    expect(screen.getByTestId("working").textContent).toBe("Working");
  });

  describe("when a step went wrong", () => {
    const call = (id: string, overrides: Partial<ToolCall>): Entry => ({
      at: 3,
      seq: Number(id.slice(1)) + 10,
      kind: "tool",
      call: {
        id,
        tool: "bash",
        input: { type: "command", command: "ls" },
        output: "",
        check: null,
        mcp: null,
        execution: { state: "finished", failed: false },
        ...overrides,
      },
    });
    const blocked = { allowed: false, reason: "no pushing" };
    const trouble: Entry[] = [
      { at: 1, seq: 1, kind: "person", said: "said", message: "Do it" },
      call("t1", {}),
      call("t2", { check: blocked, execution: null }),
      call("t3", { execution: { state: "finished", failed: true } }),
      call("t4", { execution: { state: "finished", failed: true } }),
    ];

    it("says on the closed row how many were refused and how many failed", () => {
      render(<Turn turn={turnOf(trouble)} unfolded={new Set()} onToggle={() => {}} />);
      const row = screen.getByRole("button", { name: /^Worked/ });
      expect(row.textContent).toBe("Worked · 4 steps · 1 refused · 2 failed");
      expect(screen.getByTestId("steps-trouble").className).toContain("text-error-text");
    });

    it("says nothing of trouble when every step went well", () => {
      render(<Turn turn={turnOf(worked)} {...folding} />);
      expect(screen.queryByTestId("steps-trouble")).toBeNull();
    });

    it("opens by itself, and the person can close it", () => {
      const turn = turnOf(trouble);
      const key = turn.blocks[0]?.key as string;
      const toggled: string[] = [];
      const { rerender } = render(
        <Turn turn={turn} unfolded={new Set()} onToggle={(each) => toggled.push(each)} />,
      );
      const row = screen.getByRole("button", { name: /^Worked/ });
      expect(row.getAttribute("aria-expanded")).toBe("true");
      expect(row.getAttribute("aria-controls")).toBe(screen.getByTestId("steps").id);
      fireEvent.click(row);
      expect(toggled).toEqual([`${key}:closed`]);
      rerender(<Turn turn={turn} unfolded={new Set(toggled)} onToggle={() => {}} />);
      expect(screen.queryByTestId("steps")).toBeNull();
    });
  });
});

/** A turn that holds its own folded state, as the thread does. */
function Held({ turn, on }: { turn: TurnData; on: boolean }) {
  const [unfolded, setUnfolded] = useState<ReadonlySet<string>>(new Set());
  const toggle = (key: string) =>
    setUnfolded((was) => {
      const next = new Set(was);
      if (!next.delete(key)) next.add(key);
      return next;
    });
  return (
    <ThinkingProvider value={on}>
      <Turn turn={turn} unfolded={unfolded} onToggle={toggle} />
    </ThinkingProvider>
  );
}

describe("the setting that shows thinking open", () => {
  const thought: Entry[] = [
    { at: 1, seq: 1, kind: "person", said: "said", message: "Do it" },
    { at: 2, seq: 2, kind: "thinking", text: "Let me look." },
    { at: 5, seq: 3, kind: "text", text: "Done." },
  ];
  const thoughtOf = (working: boolean) => {
    const turn = groupTurns(thought, working)[0];
    if (!turn) throw new Error("one turn");
    return turn;
  };

  it("leaves a finished thought folded when it is off", () => {
    render(<Held turn={thoughtOf(false)} on={false} />);
    expect(screen.getByRole("button", { name: /^Thought/ }).getAttribute("aria-expanded")).toBe(
      "false",
    );
    expect(screen.queryByText("Let me look.")).toBeNull();
  });

  it("opens a finished thought when it is on, and keeps a choice to close it", () => {
    render(<Held turn={thoughtOf(false)} on={true} />);
    const row = screen.getByRole("button", { name: /^Thought/ });
    expect(row.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByText("Let me look.")).toBeTruthy();
    fireEvent.click(row);
    expect(screen.queryByText("Let me look.")).toBeNull();
  });

  it("keeps a thought the person opened against the setting", () => {
    render(<Held turn={thoughtOf(false)} on={false} />);
    fireEvent.click(screen.getByRole("button", { name: /^Thought/ }));
    expect(screen.getByText("Let me look.")).toBeTruthy();
  });

  it("opens a thought that is still arriving in both settings, and cannot close it", () => {
    for (const on of [false, true]) {
      cleanup();
      const arriving: Entry[] = [
        { at: 1, seq: 1, kind: "person", said: "said", message: "Do it" },
        { at: 2, seq: 2, kind: "thinking", text: "Still thinking." },
      ];
      const turn = groupTurns(arriving, true)[0];
      if (!turn) throw new Error("one turn");
      render(
        <ThinkingProvider value={on}>
          <Turn turn={turn} {...folding} />
        </ThinkingProvider>,
      );
      const row = screen.getByRole("button", { name: /^Thinking/ });
      expect(row.getAttribute("aria-expanded")).toBe("true");
      expect(screen.getByText("Still thinking.")).toBeTruthy();
    }
  });

  it("collapses the thought once the answer has come when it is off", () => {
    const arriving: Entry[] = thought.slice(0, 2);
    const during = groupTurns(arriving, true)[0];
    const after = groupTurns(thought, false)[0];
    if (!during || !after) throw new Error("one turn");
    const { rerender } = render(<Turn turn={during} {...folding} />);
    expect(screen.getByText("Let me look.")).toBeTruthy();
    rerender(<Turn turn={after} {...folding} />);
    expect(screen.queryByText("Let me look.")).toBeNull();
  });

  it("opens a folded group of steps when it is on", () => {
    const many: Entry[] = [
      { at: 1, seq: 1, kind: "person", said: "said", message: "Do it" },
      ...[1, 2, 3, 4, 5].map(
        (n): Entry => ({
          at: 1 + n,
          seq: 1 + n,
          kind: "tool",
          call: {
            id: `m${n}`,
            tool: "bash",
            input: { type: "command", command: `ls ${n}` },
            output: "",
            check: null,
            mcp: null,
            execution: { state: "finished", failed: false },
          },
        }),
      ),
    ];
    const turn = groupTurns(many, false)[0];
    if (!turn) throw new Error("one turn");
    render(
      <ThinkingProvider value={true}>
        <Turn turn={turn} {...folding} />
      </ThinkingProvider>,
    );
    expect(
      screen.getByRole("button", { name: "Worked · 5 steps" }).getAttribute("aria-expanded"),
    ).toBe("true");
  });

  describe("the box a tool step opens to", () => {
    const entries: Entry[] = [
      { at: 1, seq: 1, kind: "person", said: "said", message: "Check" },
      {
        at: 2,
        seq: 2,
        kind: "tool",
        call: {
          id: "t1",
          tool: "bash",
          input: { type: "command", command: "cargo test" },
          output: "test result: ok",
          check: { allowed: false, reason: "no pushing" },
          mcp: null,
          execution: { state: "finished", failed: false },
        },
      },
    ];

    it("draws its words, rule, buttons and error on the dark colour of code, not on the page's text colour", () => {
      const turn = turnOf(entries);
      render(<Open turn={turn} />);
      fireEvent.click(screen.getByRole("button", { name: /Blocked/ }));
      const box = screen.getByTestId("tool-details");
      expect(box.className).toContain("bg-code-bg");
      expect(box.className).not.toMatch(/bg-code(?!-)/);
      for (const text of ["cargo test", "test result: ok"]) {
        const className = screen.getByText(text).className;
        expect(className).toContain("text-code-text");
        expect(className).not.toMatch(/text-text/);
      }
      expect(screen.getByText(/Blocked:/).className).toContain("text-code-error");
      expect(box.innerHTML).not.toMatch(/border-hairline|text-secondary|text-text/);
      for (const name of ["Copy input", "Copy output"]) {
        const className = screen.getByRole("button", { name }).className;
        expect(className).toContain("text-code-head");
        expect(className).toContain("hover:text-code-text");
        expect(className).toContain("opacity-100!");
      }
    });
  });
});
