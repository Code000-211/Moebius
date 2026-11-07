import { describe, expect, it } from "vitest";
import type { ServerSettings } from "../../api/types/settings";
import { type Change, changes, type Form, nothingToSave, readForms } from "./fields";

/** The answer of `GET /v1/settings` for a file that sets `set`; the keys it does not set have their default. */
function answer(set: Record<string, unknown>): ServerSettings {
  const defaults: Record<string, unknown> = {
    listen: "127.0.0.1:7171",
    run_limit: 3,
    "board.tick_ms": 2000,
    "board.round_limit": 5,
    "budgets.day_tokens": null,
    "trial_window.start": "00:00",
    "regression.gate": true,
    "harness.mcp_env": [],
  };
  const values: ServerSettings["values"] = {};
  for (const [key, value] of Object.entries(defaults)) values[key] = { value, default: true };
  for (const [key, value] of Object.entries(set)) values[key] = { value, default: false };
  return {
    file: "/srv/moebius/moebius.toml",
    values,
    secrets: [],
    restart_needed: false,
    restart_keys: [],
  };
}

const configured = () =>
  readForms(
    answer({
      listen: "127.0.0.1:9999",
      "board.round_limit": 7,
      "budgets.day_tokens": 8_000_000,
      retro_at: "22:30",
      "github.app_id": 12,
      "github.private_key_file": "/k.pem",
      "harness.mcp_env": ["DOCS_TOKEN"],
    }),
  );

function edit(forms: Form[], key: string, text: string): Form[] {
  return forms.map((form) => ({
    ...form,
    fields: form.fields.map((field) => (field.key === key ? { ...field, text } : field)),
  }));
}

const field = (forms: Form[], key: string) =>
  forms.flatMap((form) => form.fields).find((each) => each.key === key);

describe("reading the settings", () => {
  it("says what an empty field means where the key has no default", () => {
    const forms = readForms(answer({}));
    // A budget with no figure is no budget at all, and a forge token comes from a file or from a secret.
    expect(field(forms, "budgets.day_tokens")?.whenEmpty).toBe("No limit");
    expect(field(forms, "gitlab.token_file")?.whenEmpty).toBe("This one or the secret");
    expect(field(forms, "gitlab.token_secret")?.whenEmpty).toBe("This one or the file");
    expect(field(forms, "github.token_secret")?.whenEmpty).toBe("This one, the file or an App");
    expect(field(forms, "github.app_id")?.whenEmpty).toBe("Instead of a token, with the key file");
    // A key with a default has the default to say instead, so it needs no words of its own.
    expect(field(forms, "run_limit")?.whenEmpty).toBeNull();
    expect(field(forms, "run_limit")?.default).toBe("3");
  });

  it("holds what the file sets, and shows the default of a key it does not set", () => {
    const forms = configured();
    expect(forms.map((form) => form.title)).toEqual([
      "Server",
      "Board and budgets",
      "Nightly schedule",
      "Harness",
      "Pi-Bolt",
      "GitHub",
      "GitLab",
      "Forgejo",
    ]);
    expect(field(forms, "listen")).toMatchObject({ text: "127.0.0.1:9999", default: null });
    expect(field(forms, "run_limit")).toMatchObject({ text: "", default: "3" });
    expect(field(forms, "harness.mcp_env")?.text).toBe("DOCS_TOKEN");
    expect(field(forms, "budgets.day_tokens")?.text).toBe("8000000");
    // A picker always holds a choice.
    expect(field(forms, "regression.gate")).toMatchObject({ text: "true", default: "true" });
    // A key the server did not tell is blank without a default.
    expect(field(forms, "pi_bolt.program")).toMatchObject({ text: "", default: null });
  });
});

describe("what a save sends", () => {
  it("is nothing for nothing changed", () => {
    expect(nothingToSave(changes(configured()) as Change)).toBe(true);
  });

  it("sends exactly the keys that were changed, typed by their kind", () => {
    let forms = configured();
    forms = edit(forms, "board.round_limit", "9");
    forms = edit(forms, "budgets.day_tokens", "5000");
    forms = edit(forms, "listen", " 127.0.0.1:9000 ");
    forms = edit(forms, "harness.mcp_env", "DOCS_TOKEN, OTHER");
    forms = edit(forms, "regression.gate", "false");
    // A key put back to what the file says is not sent.
    forms = edit(forms, "retro_at", "21:00");
    forms = edit(forms, "retro_at", "22:30");
    expect(changes(forms)).toEqual({
      set: {
        "board.round_limit": 9,
        "budgets.day_tokens": 5000,
        listen: "127.0.0.1:9000",
        "harness.mcp_env": ["DOCS_TOKEN", "OTHER"],
        "regression.gate": false,
      },
      remove: [],
    });
  });

  it("removes a key whose input is blank, and a table whose inputs are all blank as a whole", () => {
    let forms = configured();
    forms = edit(forms, "budgets.day_tokens", "");
    forms = edit(forms, "board.round_limit", "");
    forms = edit(forms, "github.app_id", "");
    forms = edit(forms, "github.private_key_file", "");
    expect(changes(forms)).toEqual({
      set: {},
      remove: ["board.round_limit", "budgets.day_tokens", "github"],
    });
  });

  it("keeps the other keys of a table when one is blank", () => {
    expect(changes(edit(configured(), "github.app_id", ""))).toEqual({
      set: {},
      remove: ["github.app_id"],
    });
  });

  it("saves the name of the secret that holds the GitHub token, and not the token", () => {
    expect(changes(edit(readForms(answer({})), "github.token_secret", "github"))).toEqual({
      set: { "github.token_secret": "github" },
      remove: [],
    });
  });

  it("does not remove a table that was not in the file", () => {
    expect(nothingToSave(changes(readForms(answer({}))) as Change)).toBe(true);
  });

  it("says what is wrong with a number instead of sending it", () => {
    expect(changes(edit(configured(), "board.tick_ms", "fast"))).toEqual({
      error: "Board tick (ms) is a whole number.",
    });
    expect(changes(edit(configured(), "run_limit", "2.5"))).toEqual({
      error: "Runs at a time is a whole number.",
    });
  });
});
