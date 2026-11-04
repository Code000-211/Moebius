import { describe, expect, it } from "vitest";
import type { ServerSettings } from "../../api/types/models";
import {
  type AgentRequest,
  addPath,
  agentRequest,
  blankForm,
  copyable,
  copyOf,
  decisionChoices,
  engineeringOf,
  fileTable,
  formOf,
  fromFile,
  modelChoices,
  toggle,
  withChosen,
} from "./model";

const filled = () => ({ ...blankForm("Coder"), model: "qwen" });

describe("what a save sends", () => {
  it("is the name, the settings the agent needs and the instructions", () => {
    const form = { ...filled(), instructions: "Keep changes small.\n", skills: ["tdd"] };
    expect(agentRequest(form)).toEqual({
      name: "Coder",
      settings: { model: "qwen", protocol: "openai", skills: ["tdd"], extensions: [], mcp: [] },
      instructions: "Keep changes small.",
    });
  });

  it("leaves out what is the default and sends the rest", () => {
    const form = {
      ...filled(),
      harness: "claude-code",
      thinking: "high",
      budget: " 2000000 ",
      runner: "box",
      hideAfter: "0",
      engineeringSkills: [] as string[],
    };
    const request = agentRequest(form);
    expect(request).toMatchObject({
      settings: {
        harness: "claude-code",
        thinking: "high",
        token_budget: 2000000,
        runner: "box",
        engineering_skills: false,
        tool_output: { hide_after_turns: 0, max_lines: 200, max_bytes: 16384, compress: true },
      },
    });
  });

  it.each([
    [{ name: "bad name" }, "A name is made of letters, digits, - and _."],
    [{ name: "" }, "A name is made of letters, digits, - and _."],
    [{ model: "  " }, "Enter the model."],
    [{ budget: "0" }, "The token budget is a whole number of at least 1."],
    [{ budget: "1.5" }, "The token budget is a whole number of at least 1."],
    [{ maxLines: "many" }, "The tool output fields are whole numbers; 0 turns one off."],
  ])("says what is wrong with %j", (change, problem) => {
    expect(agentRequest({ ...filled(), ...change })).toBe(problem);
  });
});

describe("opening an agent", () => {
  it("fills the fields from the files and marks the agent as one that can be deleted", () => {
    const form = formOf({
      name: "Coder",
      instructions: "Hi.",
      settings: {
        model: "m",
        protocol: "anthropic",
        thinking: "low",
        token_budget: 5,
        skills: ["a"],
        mcp: ["docs"],
        extensions: ["extensions/log.ts"],
        engineering_skills: false,
        tool_output: { hide_after_turns: 1, max_lines: 2, max_bytes: 3, compress: false },
      },
    });
    expect(form).toMatchObject({
      exists: true,
      model: "m",
      protocol: "anthropic",
      thinking: "low",
      budget: "5",
      hideAfter: "1",
      compress: false,
      engineeringSkills: [],
      mcp: ["docs"],
    });
  });

  it("edits a built-in agent without files from blank and keeps what it brings", () => {
    const form = formOf({ name: "Moebius", built_in: { instructions: "Always.", skills: [] } });
    expect(form.exists).toBe(false);
    expect(form.builtIn?.instructions).toBe("Always.");
    expect(form.hideAfter).toBe("10");
  });
});

describe("the lists of names", () => {
  it("toggles a name and keeps the chosen names that are not on offer", () => {
    expect(toggle(["a"], "b")).toEqual(["a", "b"]);
    expect(toggle(["a", "b"], "a")).toEqual(["b"]);
    expect(withChosen(["a"], ["a", "gone"])).toEqual(["a", "gone"]);
  });

  it("adds a path once and not when blank", () => {
    expect(addPath(["a"], " b ")).toEqual(["a", "b"]);
    expect(addPath(["a"], "a")).toEqual(["a"]);
    expect(addPath(["a"], "  ")).toEqual(["a"]);
  });
});

describe("an agent of moebius.toml", () => {
  const server = (agents: unknown): ServerSettings => ({
    file: "/x/moebius.toml",
    values: { agents: { value: agents, default: false } },
    secrets: [],
    restart_needed: false,
    restart_keys: [],
  });

  it("is read from the table and written back as one", () => {
    const detail = fromFile(
      "Coder",
      server({
        Coder: {
          model: "m",
          thinking: "low",
          instructions: "Hi.",
          skills: ["skills/a"],
          token_budget: 7,
          tool_output: { max_lines: 5 },
        },
      }),
    );
    expect(detail?.settings).toMatchObject({
      model: "m",
      protocol: "openai",
      thinking: "low",
      token_budget: 7,
      skills: ["skills/a"],
      tool_output: { hide_after_turns: 10, max_lines: 5, max_bytes: 16384, compress: true },
    });
    expect(fileTable(detail?.settings as never, "Hi.")).toEqual({
      model: "m",
      protocol: "openai",
      thinking: "low",
      instructions: "Hi.",
      token_budget: 7,
      tool_output: { hide_after_turns: 10, max_lines: 5, max_bytes: 16384, compress: true },
      skills: ["skills/a"],
    });
  });

  it("is not there when the file has no such table", () => {
    expect(fromFile("Nobody", server({}))).toBeNull();
  });
});

describe("the choices of a Model field", () => {
  const available = {
    models: [{ id: "m1", protocol: "openai", upstream: "http://u" }],
    errors: [],
    classes: [
      { name: "strong", models: ["m1"], protocols: ["openai"] },
      { name: "mixed", models: ["m1", "m2"], protocols: ["openai", "anthropic"] },
    ],
    decision_models: [],
  };

  it("are the classes first, with a protocol only when there is one", () => {
    expect(modelChoices(available).map((c) => [c.id, c.protocol, c.class])).toEqual([
      ["strong", "openai", true],
      ["mixed", "", true],
      ["m1", "openai", false],
    ]);
  });

  it("are what the decision model's endpoint lists, or the upstreams' models before it lists any", () => {
    expect(decisionChoices(available, "http://d").map((c) => c.id)).toEqual(["m1"]);
    const listed = { ...available, decision_models: ["decider"] };
    expect(decisionChoices(listed, "http://d")).toEqual([
      { id: "decider", protocol: "openai", upstream: "http://d", class: false },
    ]);
  });
});

describe("starting from an existing agent", () => {
  const entry = (name: string, built_in: boolean, model = "m") =>
    ({ name, model, protocol: "openai", harness: "pi-bolt", built_in }) as const;

  it("offers the Coder and the Reviewer and the person's own agents, not the others Moebius runs", () => {
    const names = copyable([
      entry("Moebius", true),
      entry("Grilling-partner", true),
      entry("Coder", true, ""),
      entry("Reviewer", true),
      entry("Fast", false),
      { ...entry("Broken", false), error: "bad" },
    ]).map((agent) => agent.name);
    expect(names).toEqual(["Coder", "Reviewer", "Fast"]);
  });

  it("copies every setting of an agent of the person's own under a name still to be typed", () => {
    const form = copyOf(
      {
        name: "Fast",
        settings: {
          model: "small",
          protocol: "anthropic",
          thinking: "low",
          token_budget: 5,
          harness: "claude-code",
          runner: "box",
          skills: ["a"],
          extensions: ["extensions/x.ts"],
          mcp: ["docs"],
          engineering_skills: false,
          tool_output: { hide_after_turns: 0, max_lines: 1, max_bytes: 2, compress: false },
        },
        instructions: "Be brief.",
      },
      undefined,
    );
    expect(form).toMatchObject({
      name: "",
      naming: true,
      exists: false,
      builtIn: null,
      model: "small",
      protocol: "anthropic",
      thinking: "low",
      budget: "5",
      harness: "claude-code",
      runner: "box",
      skills: ["a"],
      extensions: ["extensions/x.ts"],
      mcp: ["docs"],
      engineeringSkills: [],
      hideAfter: "0",
      compress: false,
      instructions: "Be brief.",
    });
  });

  it("copies a built-in agent with the settings it runs with and its built-in instructions as its own", () => {
    const built = { instructions: "You are the Coder.", skills: [] };
    const form = copyOf(
      { name: "Coder", built_in: built },
      { ...entry("Coder", true, "big"), protocol: "anthropic", thinking: "high" },
    );
    expect(form).toMatchObject({
      name: "",
      naming: true,
      builtIn: null,
      model: "big",
      protocol: "anthropic",
      thinking: "high",
      instructions: "You are the Coder.",
    });
    // Its own instructions come after the built-in ones.
    const own = copyOf(
      {
        name: "Coder",
        built_in: built,
        settings: { model: "m", protocol: "openai", skills: [], extensions: [], mcp: [] },
        instructions: "Prefer small scripts.",
      },
      entry("Coder", true),
    );
    expect(own.instructions).toBe("You are the Coder.\n\nPrefer small scripts.");
  });

  it("gives a copy the engineering skills its source has, as a list of its own", () => {
    const built = {
      instructions: "You are the Coder.",
      skills: [],
      engineering_skills: ["tdd", "diagnosing-bugs", "moebius-autonomous"].map((name) => ({
        name,
        description: "",
      })),
    };
    const copy = copyOf({ name: "Coder", built_in: built }, entry("Coder", true, "big"));
    expect(copy.engineeringSkills).toEqual(["tdd", "diagnosing-bugs", "moebius-autonomous"]);
    const request = agentRequest({ ...copy, name: "Coder-fast", model: "small" });
    expect(request).toMatchObject({
      settings: { engineering_skills: ["tdd", "diagnosing-bugs", "moebius-autonomous"] },
    });
    // A list of the source's own is kept, and an agent with no engineering skills stays without.
    const own = copyOf(
      {
        name: "Coder",
        built_in: built,
        settings: {
          model: "m",
          protocol: "openai",
          skills: [],
          extensions: [],
          mcp: [],
          engineering_skills: ["tdd"],
        },
      },
      undefined,
    );
    expect(own.engineeringSkills).toEqual(["tdd"]);
    expect(copyOf({ name: "Fast" }, entry("Fast", false, "m")).engineeringSkills).toBeNull();
  });

  it("reads and writes the engineering skills as the default, none or a list", () => {
    const settings = { model: "m", protocol: "openai", skills: [], extensions: [], mcp: [] };
    const form = (engineering_skills?: boolean | string[]) =>
      formOf({ name: "Planner", settings: { ...settings, engineering_skills } });
    expect(form().engineeringSkills).toBeNull();
    expect(form(true).engineeringSkills).toBeNull();
    expect(form(false).engineeringSkills).toEqual([]);
    expect(form(["tdd"]).engineeringSkills).toEqual(["tdd"]);
    const sent = (engineeringSkills: string[] | null) =>
      (agentRequest({ ...form(), engineeringSkills }) as AgentRequest).settings.engineering_skills;
    expect(sent(null)).toBeUndefined();
    expect(sent([])).toBe(false);
    expect(sent(["tdd", "moebius-autonomous"])).toEqual(["tdd", "moebius-autonomous"]);
    expect(engineeringOf({ ...form(), name: "Reviewer" })).toEqual([
      "code-review",
      "moebius-autonomous",
    ]);
    expect(engineeringOf({ ...form(["tdd"]), name: "Reviewer" })).toEqual(["tdd"]);
  });
});
