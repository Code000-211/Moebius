import { describe, expect, it } from "vitest";
import { ApiError } from "../api/http";
import type { InboxItem } from "../api/types/board";
import type { PackageOutcome, PiOffered, PiUpdates } from "../api/types/packages";
import {
  changeLink,
  changelogs,
  inKind,
  installsAtOnce,
  kindLabel,
  needsRepository,
  piInKind,
  piSource,
  refusal,
  standing,
  tellOffered,
  tellOutcome,
  tellUpdates,
  waitingPiUpdates,
} from "./model";

const outcome = (extra: Partial<PackageOutcome> = {}): PackageOutcome => ({
  name: "gate",
  version: "1.1.0",
  kind: "plugin",
  action: "installed",
  restart_needed: false,
  ...extra,
});

const offered = (extra: Partial<PiOffered> = {}): PiOffered => ({
  change: {
    id: 41,
    repository_id: null,
    harness_repository: true,
    card_id: null,
    branch: "moebius/ext-add-x-41",
    state: "open",
    pull_request: null,
    checks: null,
    review: null,
    head: null,
  } as PiOffered["change"],
  package: "pi-notes",
  action: "add",
  source: "npm:pi-notes",
  from: null,
  to: "0.4.0",
  resources: { extensions: [], skills: [], prompts: [], themes: [] },
  agents: [],
  changelog: "",
  files: [],
  dependency_files: 0,
  diff: "",
  ...extra,
});

describe("what an install of a Moebius package tells", () => {
  it("says what was installed, as the CLI does", () => {
    expect(tellOutcome(outcome())).toEqual({
      tone: "ok",
      lines: ["Installed gate 1.1.0."],
      change: null,
    });
  });

  it("says the plugin's own error when it does not run, and that the server must restart", () => {
    const told = tellOutcome(
      outcome({
        restart_needed: true,
        plugin: {
          program: "/p/gate",
          state: "restarting",
          restarts: 1,
          error: "exited with status 1",
        },
      }),
    );
    expect(told.tone).toBe("error");
    expect(told.lines).toEqual([
      "Installed gate 1.1.0.",
      "gate does not run: exited with status 1",
      "Restart the server for it to take effect.",
    ]);
  });

  it("names the change that proposes the files of a package that is not a plugin", () => {
    const told = tellOutcome(outcome({ name: "review-flow", kind: "workflow", change: 12 }));
    expect(told.change).toBe(12);
    expect(told.lines.at(-1)).toContain("a person merges");
  });

  it("says an update that found nothing newer, an update, and a removal with a folder left over", () => {
    expect(tellOutcome(outcome({ action: "unchanged" })).lines[0]).toContain("is the newest");
    expect(tellOutcome(outcome({ action: "updated" })).lines[0]).toBe("Updated gate to 1.1.0.");
    const removed = tellOutcome(outcome({ action: "removed", leftover: "/srv/p/gate" }));
    expect(removed.lines).toEqual([
      "Removed gate.",
      "Its folder could not be deleted: /srv/p/gate. Delete it by hand.",
    ]);
  });
});

describe("what a removal or an unchanged update proposes", () => {
  it("says a removal that carries a change is proposed, not done", () => {
    const told = tellOutcome(
      outcome({ name: "review-flow", kind: "workflow", action: "removed", change: 13 }),
    );
    expect(told.lines[0]).toBe("Proposed removing review-flow.");
    expect(told.lines.at(-1)).toContain("proposed as change 13");
    expect(told.change).toBe(13);
  });

  it("does not propose files for an update that found nothing newer", () => {
    const told = tellOutcome(outcome({ kind: "workflow", action: "unchanged", change: 12 }));
    expect(told.lines).toHaveLength(1);
    expect(told.lines.join(" ")).not.toContain("proposed as change");
    expect(told.change).toBeNull();
  });
});

describe("what a proposal for a Pi package tells", () => {
  it("says nothing changes until a person merges, and links the change", () => {
    const told = tellOffered(offered({ agents: ["Coder"] }));
    expect(told.change).toBe(41);
    expect(told.lines).toEqual([
      "Proposed installing pi-notes 0.4.0. Nothing changes until a person merges it.",
      "It is named for Coder.",
    ]);
  });

  it("says an update's versions and a removal's agents", () => {
    expect(
      tellOffered(offered({ action: "update", from: "0.3.0", to: "0.4.0" })).lines[0],
    ).toContain("from 0.3.0 to 0.4.0");
    const removal = tellOffered(
      offered({ action: "remove", from: "0.3.0", to: null, agents: ["Coder"] }),
    );
    expect(removal.lines).toContain("Coder loses its names.");
  });

  it("tells each update, the ones up to date and the one that failed", () => {
    const updates: PiUpdates = {
      offered: [offered({ action: "update", from: "0.3.0", changelog: "## 0.4.0\n- fix\n" })],
      up_to_date: ["pi-other"],
      failed: [{ package: "pi-broken", error: "the registry answered 503" }],
    };
    const told = tellUpdates(updates);
    expect(told.map((each) => each.tone)).toEqual(["ok", "ok", "error"]);
    expect(told[1]?.lines).toEqual(["pi-other is up to date."]);
    expect(told[2]?.lines[0]).toContain("the registry answered 503");
    expect(changelogs(updates)).toEqual([{ name: "pi-notes", text: "## 0.4.0\n- fix" }]);
  });
});

describe("what a refusal says", () => {
  it("uses the server's sentence", () => {
    expect(refusal(new ApiError(422, "gate 9.0.0 works with Moebius >=9.0"), "x")).toBe(
      "gate 9.0.0 works with Moebius >=9.0",
    );
  });

  it("says whose it is when the session is not a person's", () => {
    const sentence = refusal(new ApiError(403, "only a person can install a plugin"), "x");
    expect(sentence).toContain("this session is not a person's");
  });

  it("says it once when the server's sentence says the same", () => {
    const sentence = refusal(new ApiError(403, "only a person can install a plugin"), "x");
    expect(sentence.match(/person/gi)).toHaveLength(2);
    expect(sentence).not.toContain("only a person can install");
    const other = refusal(new ApiError(403, "the token has no write scope"), "x");
    expect(other).toBe(
      "Only a person can do this, and this session is not a person's. the token has no write scope",
    );
  });

  it("falls back for a failure with no sentence", () => {
    expect(refusal(new Error("boom"), "Could not install it.")).toBe("Could not install it.");
  });
});

describe("the catalogs", () => {
  it("installs a plugin at once and proposes the files of the others", () => {
    expect(installsAtOnce("plugin")).toBe(true);
    for (const kind of ["workflow", "rules", "skills"] as const) {
      expect(installsAtOnce(kind)).toBe(false);
    }
    expect(needsRepository("workflow")).toBe(true);
    expect(needsRepository("skills")).toBe(false);
  });

  it("filters by kind", () => {
    const list = [
      { kind: "plugin", name: "a" },
      { kind: "rules", name: "b" },
    ] as never[];
    expect(inKind(list, "rules")).toHaveLength(1);
    expect(inKind(list, null)).toHaveLength(2);
    const hits = [{ kinds: ["extension", "skill"] }, { kinds: ["theme"] }] as never[];
    expect(piInKind(hits, "skill")).toHaveLength(1);
  });

  it("names a source, a link and a kind", () => {
    expect(piSource("@acme/pi-notes")).toBe("npm:@acme/pi-notes");
    expect(changeLink(41)).toBe("/inbox?change=41");
    expect(kindLabel("rules")).toBe("Rules");
    expect(kindLabel("unheard-of")).toBe("unheard-of");
  });

  it("marks a package installed, or with an update waiting", () => {
    expect(standing(null, "1.0.0")).toEqual([]);
    expect(standing("1.0.0", "1.0.0")).toEqual([{ label: "Installed 1.0.0", tone: "complete" }]);
    expect(standing("1.0.0", "1.1.0")).toEqual([
      { label: "Installed 1.0.0", tone: "complete" },
      { label: "Update to 1.1.0", tone: "attention" },
    ]);
    expect(standing("1.0.0", null).map((each) => each.label)).toEqual(["Installed 1.0.0"]);
  });
});

describe("the updates the nightly check left in the inbox", () => {
  const item = (kind: string, data: Record<string, unknown>): InboxItem => ({
    id: 1,
    kind,
    card_id: null,
    title: "t",
    data,
    resolved_at: null,
    at: 1,
  });

  it("reads the Pi ones by package and ignores the rest", () => {
    const waiting = waitingPiUpdates([
      item("extension_update", { package: "pi-notes", from: "0.3.0", to: "0.4.0" }),
      item("package_update", { name: "gate", installed: "1.0.0", version: "1.1.0" }),
      item("extension_update", { from: "1" }),
    ]);
    expect([...waiting.entries()]).toEqual([["pi-notes", { from: "0.3.0", to: "0.4.0" }]]);
  });
});
