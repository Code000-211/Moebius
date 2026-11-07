// @vitest-environment jsdom
/**
 * The block that says what went wrong holds the way back, when the page has one to offer: the chat whose agent went
 * away is answered again from the words it was answering, and the failure is where the person is already looking.
 */
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { Entry, ToolCall } from "../../chat/conversation";
import { groupTurns } from "../../chat/turns";
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

  it("is one folded row that counts the steps, and shows none of them", () => {
    render(<Turn turn={turnOf(worked)} {...folding} />);
    const row = screen.getByRole("button", { name: "Worked · 2 steps" });
    expect(row.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByTestId("steps")).toBeNull();
  });

  it("holds the steps once it is unfolded", () => {
    const turn = turnOf(worked);
    const key = turn.blocks[0]?.key as string;
    render(<Turn turn={turn} unfolded={new Set([key])} onToggle={() => {}} />);
    expect(screen.getByTestId("steps").textContent).toContain("Thought");
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
