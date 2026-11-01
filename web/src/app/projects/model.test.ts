import { describe, expect, it } from "vitest";
import type { Repository } from "../../api/types/board";
import { rank, score } from "./fuzzy";
import {
  cardsText,
  EMPTY_FORM,
  forgeTag,
  lineStart,
  logins,
  notEditable,
  onForge,
  parents,
  repositoryRequest,
} from "./model";

const repository = (extra: Partial<Repository>): Repository => ({
  id: 1,
  name: "radiance",
  path: "/r",
  forge: "none",
  default_branch: "main",
  github: null,
  trusted: [],
  ...extra,
});

describe("how a repository is told", () => {
  it("names the forge and the repository there", () => {
    expect(forgeTag(repository({}))).toBe("No forge");
    expect(forgeTag(repository({ forge: "github", github: "acme/radiance" }))).toBe(
      "GitHub · acme/radiance",
    );
    expect(forgeTag(repository({ forge: "gitlab", project: "acme/platform/tools" }))).toBe(
      "GitLab · acme/platform/tools",
    );
    expect(forgeTag(repository({ forge: "forgejo", project: "acme/gadgets" }))).toBe(
      "Forgejo · acme/gadgets",
    );
    expect(onForge(repository({}))).toBe(false);
    expect(onForge(repository({ forge: "gitlab", project: "a/b" }))).toBe(true);
    expect(cardsText(1)).toBe("1 card");
    expect(cardsText(0)).toBe("0 cards");
  });
});

describe("the form of a new repository", () => {
  const filled = { ...EMPTY_FORM, name: " radiance ", url: " /srv/git/radiance " };

  it("is not sent until the name, the address and the forge's name are there", () => {
    expect(repositoryRequest(EMPTY_FORM)).toBeNull();
    expect(repositoryRequest({ ...filled, url: "" })).toBeNull();
    expect(repositoryRequest({ ...filled, forge: "github" })).toBeNull();
    expect(repositoryRequest({ ...filled, forge: "gitlab", github: "a/b" })).toBeNull();
  });

  it("sends a repository without a forge with nothing else", () => {
    expect(repositoryRequest({ ...filled, github: "left", trusted: "leon" })).toEqual({
      name: "radiance",
      url: "/srv/git/radiance",
      forge: "none",
    });
  });

  it("sends the repository of the forge and the trusted logins, trimmed", () => {
    expect(
      repositoryRequest({
        ...filled,
        forge: "github",
        github: " acme/radiance ",
        trusted: "leon, mona",
      }),
    ).toEqual({
      name: "radiance",
      url: "/srv/git/radiance",
      forge: "github",
      github: "acme/radiance",
      trusted: ["leon", "mona"],
    });
    expect(
      repositoryRequest({ ...filled, forge: "forgejo", project: "acme/gadgets", github: "left" }),
    ).toEqual({
      name: "radiance",
      url: "/srv/git/radiance",
      forge: "forgejo",
      project: "acme/gadgets",
      trusted: [],
    });
  });

  it("reads logins separated by commas, spaces or lines", () => {
    expect(logins("leon, mona\nkim  ,")).toEqual(["leon", "mona", "kim"]);
    expect(logins("  ")).toEqual([]);
  });
});

describe("files", () => {
  it("says why a file is not edited", () => {
    expect(notEditable({ path: "logo.png", size: 48211, binary: true })).toBe(
      "logo.png is binary (48211 bytes), not editable here.",
    );
    expect(notEditable({ path: "data.bin", size: 2000000, too_large: true })).toBe(
      "data.bin is over 1 MB (2000000 bytes), not editable here.",
    );
    expect(notEditable({ path: "a.rs", size: 3, text: "abc" })).toBeNull();
  });

  it("lists the directories above a path", () => {
    expect(parents("src/server/main.rs")).toEqual(["src", "src/server"]);
    expect(parents("README.md")).toEqual([]);
  });

  it("finds where a line starts", () => {
    const text = "one\ntwo\nthree";
    expect(lineStart(text, 1)).toBe(0);
    expect(lineStart(text, 3)).toBe(8);
    expect(lineStart(text, 9)).toBe(text.length);
  });
});

describe("go to file", () => {
  const paths = [
    "src/main.rs",
    "src/server/main.rs",
    "src/remain.rs",
    "docs/mermaid.md",
    "Cargo.toml",
  ];

  it("matches the characters in order, ignoring case, and nothing else", () => {
    expect(score("mr", "src/main.rs")).not.toBeNull();
    expect(score("MR", "src/main.rs")).not.toBeNull();
    expect(score("nm", "src/main.rs")).toBeNull();
    expect(score("  ", "anything")).toBe(0);
  });

  it("lists the best matches first: the shorter path, the match in the name, a file named as typed", () => {
    const matches = rank("mr", paths, 10);
    expect(matches.slice(0, 2)).toEqual(["src/main.rs", "src/server/main.rs"]);
    expect(matches).toHaveLength(4);
    expect(matches).not.toContain("Cargo.toml");
    expect(rank("cargo", paths, 10)[0]).toBe("Cargo.toml");
    expect(rank("main", paths, 10)[0]).toBe("src/main.rs");
    expect(rank("zzz", paths, 10)).toEqual([]);
    expect(rank("", paths, 2)).toHaveLength(2);
  });
});
