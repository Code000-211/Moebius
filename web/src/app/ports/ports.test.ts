import { describe, expect, it } from "vitest";
import type { Port } from "../../api/types/runs";
import { forwardCommand, owner, ownerPath } from "./ports";

const port = (fields: Partial<Port>): Port => ({
  port: 3000,
  pid: 1,
  process: "node",
  kind: "run",
  run: 4,
  ...fields,
});

describe("owner", () => {
  it("names what the port belongs to", () => {
    expect(owner(port({}))).toBe("run 4");
    expect(owner(port({ kind: "chat", run: 9 }))).toBe("chat 9");
    expect(owner(port({ kind: "terminal", run: 4 }))).toBe("terminal of run 4");
    expect(owner(port({ kind: "terminal", run: null }))).toBe("server terminal");
  });
});

describe("ownerPath", () => {
  it("leads to the run or the chat, and nowhere for the server's terminal", () => {
    expect(ownerPath(port({}))).toBe("/runs/4");
    expect(ownerPath(port({ kind: "terminal", run: 4 }))).toBe("/runs/4");
    expect(ownerPath(port({ kind: "chat", run: 9 }))).toBe("/chat/9");
    expect(ownerPath(port({ kind: "terminal", run: null }))).toBeNull();
  });
});

describe("forwardCommand", () => {
  it("forwards the port to the same port here", () => {
    expect(forwardCommand(5173, "build.example")).toBe(
      "ssh -N -L 5173:127.0.0.1:5173 build.example",
    );
  });
});
