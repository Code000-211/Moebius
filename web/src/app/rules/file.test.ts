import { describe, expect, it } from "vitest";
import { agentRequest, appendRule, refusalsLabel, replaceRule, section, validName } from "./file";

describe("a rule in its file", () => {
  const file = "Intro.\n\n## a\none\n\n## b\ntwo\n";

  it("is put back where it was, appended or dropped", () => {
    expect(replaceRule(file, "a", "## a\nfirst\nsecond\n\n\n")).toBe(
      "Intro.\n\n## a\nfirst\nsecond\n\n## b\ntwo\n",
    );
    expect(replaceRule(file, "b", "## b\nlast")).toBe("Intro.\n\n## a\none\n\n## b\nlast\n");
    expect(replaceRule("## a\none", "a", "## renamed\nnew")).toBe("## renamed\nnew\n");
    expect(replaceRule(file, "a", "")).toBe("Intro.\n\n## b\ntwo\n");
    expect(replaceRule("## a\none\n", "a", "")).toBe("");
    expect(replaceRule(file, "c", "## c\nx")).toBeNull();
    expect(appendRule(file, "## c\nthree")).toBe(
      "Intro.\n\n## a\none\n\n## b\ntwo\n\n## c\nthree\n",
    );
    expect(appendRule("", "## c\nthree")).toBe("## c\nthree\n");
  });

  it("is a heading and its text", () => {
    expect(section(" ask-first ", "Ask.\n\n")).toBe("## ask-first\nAsk.");
  });
});

describe("a rule's name", () => {
  it("has no spaces", () => {
    expect(validName(" no-vendoring ")).toBe(true);
    expect(validName("two words")).toBe(false);
    expect(validName("  ")).toBe(false);
  });
});

describe("the request to the Moebius agent", () => {
  it("tells it to keep the existing rules and save the file", () => {
    const asked = agentRequest("never touch the lockfile");
    expect(asked).toContain('`"$MOEBIUS" rules`');
    expect(asked).toContain('`"$MOEBIUS" rules save <file>`');
    expect(asked).toContain("never weaken");
    expect(asked.endsWith("What should be allowed or forbidden:\n\nnever touch the lockfile")).toBe(
      true,
    );
  });
});

describe("the refusals of a rule", () => {
  it("are counted in words", () => {
    const refusals = { a: 1, b: 3 };
    expect(refusalsLabel(refusals, "a")).toBe("1 refusal this week");
    expect(refusalsLabel(refusals, "b")).toBe("3 refusals this week");
    expect(refusalsLabel(refusals, "c")).toBe("No refusals this week");
  });
});
