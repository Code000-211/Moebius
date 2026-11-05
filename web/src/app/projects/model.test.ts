import { describe, expect, it } from "vitest";
import type { Repository } from "../../api/types/board";
import { rank, score } from "./fuzzy";
import {
  addFailure,
  cardsText,
  EMPTY_FORM,
  firstLine,
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

describe("what the server says about a repository it refused", () => {
  const clone = (stderr: string) =>
    `git clone --quiet -- https://github.com/acme/radiance.git /data/repos/radiance failed: ${stderr}`;

  it("leads with what happened when a private repository over HTTPS cannot be signed in to", () => {
    const message = clone(
      "fatal: could not read Username for 'https://github.com': terminal prompts disabled",
    );
    const failure = addFailure(message);
    expect(failure.headline).toBe("The server cannot sign in to GitHub.");
    expect(failure.next).toBe(
      "Set up the GitHub App under Settings and restart the server, then try again.",
    );
    expect(failure.field).toBe("url");
    // The command line is there to open, and is not what the person reads first.
    expect(failure.detail).toBe(message);
  });

  it("names the forge a token belongs to", () => {
    const message =
      "git clone --quiet -- https://gitlab.com/acme/radiance.git /r failed: " +
      "remote: HTTP Basic: Access denied. fatal: Authentication failed";
    expect(addFailure(message).headline).toBe("The server cannot sign in to GitLab.");
    expect(addFailure(message).next).toContain("GitLab token");
    expect(addFailure(message).next).toContain("restart the server");
  });

  it("says when the address has nothing at it, and that a private one reads the same", () => {
    const failure = addFailure(clone("remote: Repository not found."));
    expect(failure.headline).toBe("No repository at that address.");
    expect(failure.next).toContain("cannot sign in");
  });

  it("separates an address it cannot reach from one it cannot sign in to", () => {
    const failure = addFailure(
      "git clone --quiet -- https://git.acme/radiance.git /r failed: " +
        "fatal: unable to access 'https://git.acme/radiance.git/': Connection timed out",
    );
    expect(failure.headline).toBe("The server could not reach that address.");
  });

  it("has an answer for a run that failed for a reason it does not know", () => {
    const failure = addFailure("git clone --quiet -- /srv/git/r /r failed: fatal: early EOF");
    expect(failure.headline).toBe("The server could not clone this repository.");
    expect(failure.detail).toContain("early EOF");
  });

  it("passes on a sentence that is already plain, under the field it is about", () => {
    const taken = addFailure("a repository named demo exists already");
    expect(taken.headline).toBe("a repository named demo exists already");
    expect(taken.field).toBe("name");
    expect(taken.detail).toBeNull();

    const folder = addFailure(
      "/srv/git/radiance has files but is not a git repository; run git init there, or give an empty folder",
    );
    expect(folder.field).toBe("url");
    expect(folder.headline).toContain("git init");
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

describe("firstLine", () => {
  it("is the first line that says something, cut short", () => {
    expect(firstLine("\n\nPrefer small functions.\nMore.\n")).toBe("Prefer small functions.");
    expect(firstLine("")).toBe("(empty)");
    expect(firstLine("x".repeat(100))).toBe(`${"x".repeat(79)}…`);
  });
});
