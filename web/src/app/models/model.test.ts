import { describe, expect, it } from "vitest";
import type { ServerSettings } from "../../api/types/models";
import {
  blankOther,
  blankUpstream,
  type ModelsForm,
  modelsChanges,
  modelsDirty,
  otherUpstream,
  presetUpstream,
  readForm,
} from "./model";

const DEFAULTS: Record<string, unknown> = {
  "router.upstreams": [],
  "router.classes": {},
  "teacher.model": null,
  "teacher.protocol": "openai",
  "teacher.timeout_seconds": 20,
  "decision_model.url": null,
  "decision_model.model": null,
  "decision_model.api_key_env": null,
  "decision_model.api_key_secret": null,
  "decision_model.timeout_seconds": 20,
  "decision_model.shadow": null,
  "decision_model.agreement": 0.95,
  "decision_model.min_judgements": 100,
};

function settings(set: Record<string, unknown>, secrets: string[] = []): ServerSettings {
  const values: ServerSettings["values"] = {};
  for (const [key, value] of Object.entries(DEFAULTS)) values[key] = { value, default: true };
  for (const [key, value] of Object.entries(set)) values[key] = { value, default: false };
  return { file: "/srv/moebius.toml", values, secrets, restart_needed: false, restart_keys: [] };
}

const configured = () =>
  settings(
    {
      "router.upstreams": [
        { protocol: "openai", base_url: "http://127.0.0.1:8080" },
        {
          protocol: "anthropic",
          base_url: "https://api.anthropic.com",
          api_key_secret: "anthropic",
        },
        {
          protocol: "openai",
          base_url: "https://api.example.com/v1",
          api_key_env: "EXAMPLE_API_KEY",
        },
      ],
      "router.classes": {
        strong: [{ model: "claude-opus-fake", day_tokens: 5000000 }, "qwen3.8-flash-next"],
      },
      "teacher.model": "qwen3.8-flash-next",
      "decision_model.url": "http://127.0.0.1:8081/v1",
      "decision_model.model": "moebius-decider",
      "decision_model.api_key_env": "DECISION_MODEL_KEY",
    },
    ["anthropic"],
  );

/** The row at `place`; a test that asks for one that is not there fails. */
function row<T>(list: T[], place: number): T {
  const found = list[place];
  if (found === undefined) throw new Error(`no row ${place}`);
  return found;
}

const plan = (form: ModelsForm, secrets = ["anthropic"]) => modelsChanges(form, secrets);

describe("the form of the Models page", () => {
  it("sends nothing when nothing was changed", () => {
    const form = readForm(configured());
    expect(plan(form)).toEqual({ store: [], set: {}, remove: [] });
    expect(modelsDirty(form)).toBe(false);
  });

  it("stores a typed key of a new upstream under a new name and sets the row at the end", () => {
    const form = readForm(configured());
    form.upstreams.push({
      ...blankUpstream(),
      baseUrl: "https://api.new.example",
      protocol: "anthropic",
    });
    row(form.upstreams, 3).key.mode = "typed";
    expect(plan(form)).toBe("Type the key of the upstream 4.");
    row(form.upstreams, 3).key.typed = "sk-typed-secret";
    expect(plan(form)).toEqual({
      store: [["upstream-1", "sk-typed-secret"]],
      set: {
        "router.upstreams.3": {
          protocol: "anthropic",
          base_url: "https://api.new.example",
          api_key_secret: "upstream-1",
        },
      },
      remove: [],
    });
    expect(modelsDirty(form)).toBe(true);
  });

  it("replaces a stored key under its name and sends nothing else of the row again", () => {
    const form = readForm(configured());
    row(form.upstreams, 1).key.typed = "sk-new";
    expect(plan(form)).toEqual({
      store: [["anthropic", "sk-new"]],
      set: {
        "router.upstreams.1": {
          protocol: "anthropic",
          base_url: "https://api.anthropic.com",
          api_key_secret: "anthropic",
        },
      },
      remove: [],
    });
  });

  it("removes a row by its place and sets a changed one at the place it has then", () => {
    const form = readForm(configured());
    form.upstreams.splice(0, 1);
    row(form.upstreams, 1).key.env = "NEW_KEY";
    expect(plan(form)).toEqual({
      store: [],
      set: {
        "router.upstreams.1": {
          protocol: "openai",
          base_url: "https://api.example.com/v1",
          api_key_env: "NEW_KEY",
        },
      },
      remove: ["router.upstreams.0"],
    });
  });

  it("removes the teacher when it is turned off and moves the decision model's key to a typed one", () => {
    const form = readForm(configured());
    form.teacher.on = false;
    const timeout = form.decisionModel.fields.find(
      (each) => each.key === "decision_model.timeout_seconds",
    );
    if (!timeout || !form.decisionModel.key)
      throw new Error("the decision model has no such field");
    timeout.text = "9";
    form.decisionModel.key.now.mode = "typed";
    form.decisionModel.key.now.typed = "dm-key";
    expect(plan(form)).toEqual({
      store: [["decision-model", "dm-key"]],
      set: {
        "decision_model.timeout_seconds": 9,
        "decision_model.api_key_secret": "decision-model",
      },
      remove: ["teacher", "decision_model.api_key_env"],
    });
  });

  it("sends a table that was off and is turned on whole, and needs its model", () => {
    const form = readForm(settings({}));
    form.teacher.on = true;
    expect(plan(form, [])).toBe("Enter the model of the teacher.");
    row(form.teacher.fields, 0).text = "judge";
    expect(plan(form, [])).toMatchObject({
      set: { "teacher.model": "judge", "teacher.protocol": "openai" },
    });
    form.decisionModel.on = true;
    expect(plan(form, [])).toBe("Enter the model of the decision model.");
  });

  it("says when a number is not one", () => {
    const form = readForm(configured());
    row(form.teacher.fields, 2).text = "soon";
    expect(plan(form)).toBe("Time limit (s) is a whole number.");
  });

  it("refuses an upstream without a base URL", () => {
    const form = readForm(configured());
    form.upstreams.push(blankUpstream());
    expect(plan(form)).toBe("Enter the base URL of each upstream.");
  });

  it("writes the classes whole and keeps a model's tokens for the day while it stays", () => {
    const form = readForm(configured());
    row(form.classes, 0).models = "qwen3.8-flash-next, claude-opus-fake";
    form.classes.push({ uid: 0, name: "", models: "", dayTokens: [] });
    expect(plan(form)).toBe("Enter the name of each model class.");
    row(form.classes, 1).name = "fast";
    expect(plan(form)).toBe("Enter the models of the class fast.");
    row(form.classes, 1).models = " small ,";
    expect(plan(form)).toEqual({
      store: [],
      remove: ["router.classes"],
      set: {
        "router.classes": {
          strong: ["qwen3.8-flash-next", { model: "claude-opus-fake", day_tokens: 5000000 }],
          fast: ["small"],
        },
      },
    });
    row(form.classes, 1).name = "strong";
    expect(plan(form)).toBe("There are two classes named strong.");
  });

  it("takes the classes out of the file when none is left", () => {
    const form = readForm(configured());
    form.classes = [];
    expect(plan(form)).toEqual({ store: [], set: {}, remove: ["router.classes"] });
  });

  it("keeps the oauth table of an upstream that logs in with OAuth", () => {
    const oauth = { preset: "anthropic" };
    const form = readForm(
      settings({
        "router.upstreams": [
          { protocol: "anthropic", base_url: "https://api.anthropic.com", oauth },
        ],
      }),
    );
    row(form.upstreams, 0).baseUrl = "https://proxy.example.com";
    expect(plan(form, [])).toMatchObject({
      set: { "router.upstreams.0": { base_url: "https://proxy.example.com", oauth } },
    });
  });
});

describe("logging in to a provider", () => {
  it("adds the upstream of a built-in provider with its preset", () => {
    expect(
      presetUpstream({
        name: "anthropic",
        protocol: "anthropic",
        base_url: "https://api.anthropic.com",
      }),
    ).toEqual({
      protocol: "anthropic",
      base_url: "https://api.anthropic.com",
      oauth: { preset: "anthropic" },
    });
  });

  it("says what is missing in the fields of Other, with the url of its flow", () => {
    const other = blankOther();
    expect(otherUpstream(other, 1)).toBe("Base URL is empty.");
    Object.assign(other, {
      baseUrl: "https://api.example.com",
      clientId: "id",
      url: "https://x/auth",
    });
    expect(otherUpstream(other, 1)).toBe("Token URL is empty.");
    other.tokenUrl = "https://x/token";
    other.scopes = "a  b";
    expect(otherUpstream(other, 7)).toEqual({
      protocol: "openai",
      base_url: "https://api.example.com",
      oauth: {
        flow: "code",
        client_id: "id",
        token_url: "https://x/token",
        secret: "login-7",
        authorize_url: "https://x/auth",
        scopes: ["a", "b"],
      },
    });
    other.flow = "device";
    other.url = "";
    expect(otherUpstream(other, 7)).toBe("Device URL is empty.");
  });
});
