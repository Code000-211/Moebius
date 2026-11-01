import { describe, expect, it } from "vitest";
import { scriptProblem } from "./scriptError";

describe("the check of a refused script", () => {
  it("names the line of a syntax error", () => {
    expect(
      scriptProblem(
        `syntax error: [string "ready"]:4: Expected ')' (to close '(' at line 2), got <eof>`,
      ),
    ).toEqual({
      line: 4,
      message: "syntax error: Expected ')' (to close '(' at line 2), got <eof>",
      full: `syntax error: [string "ready"]:4: Expected ')' (to close '(' at line 2), got <eof>`,
    });
  });

  it("names the first line of a runtime error and keeps the traceback in the whole", () => {
    const answer = `runtime error: [string "code"]:2: attempt to index nil with 'foo'\nstack traceback:\n\t[C]: in ?\n\t[string "code"]:2: in function <[string "code"]:1>\n`;
    const problem = scriptProblem(answer);
    expect(problem.line).toBe(2);
    expect(problem.message).toBe("runtime error: attempt to index nil with 'foo'");
    expect(problem.full).toContain("stack traceback:");
  });

  it("has no line when the message names none", () => {
    expect(scriptProblem('no column "Nope"; the columns are Backlog, Not started')).toEqual({
      line: null,
      message: 'no column "Nope"; the columns are Backlog, Not started',
      full: 'no column "Nope"; the columns are Backlog, Not started',
    });
  });
});
