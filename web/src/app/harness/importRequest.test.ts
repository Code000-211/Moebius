import { describe, expect, it } from "vitest";
import { importRequest, instructionsRequest, updateRequest } from "./importRequest";

describe("importing skills", () => {
  it("unzips a zip, saves each skill without replacing one and ends with the note", () => {
    const path = "/srv/moebius/uploads/ab12.zip";
    const asked = importRequest("skills", { zip: path }, "only the pdf skill");
    expect(asked).toContain("`/srv/moebius/uploads/ab12.zip`");
    expect(asked).toContain("unzip -q /srv/moebius/uploads/ab12.zip -d import");
    expect(asked).toContain('`"$MOEBIUS" api GET /v1/skills`');
    expect(asked).toContain('`"$MOEBIUS" api PUT /v1/skills/<name> --body body.json`');
    expect(asked).toContain('`"$MOEBIUS" api PUT /v1/skills/<name>/files/<path> --body body.json`');
    expect(asked).toContain("never replace an existing skill without saying so");
    expect(asked).toContain("built-in name");
    expect(asked).toContain("Settings page");
    expect(asked.endsWith("What the person said:\n\nonly the pdf skill")).toBe(true);
  });
});

describe("importing MCP servers", () => {
  it("clones a link and takes keys as names", () => {
    const asked = importRequest("mcp", { link: "https://github.com/acme/docs-mcp" }, "");
    expect(asked).toContain("git clone --depth 1 https://github.com/acme/docs-mcp import");
    expect(asked).toContain('`"$MOEBIUS" api PUT /v1/mcp-servers/<name> --body body.json`');
    // biome-ignore lint/suspicious/noTemplateCurlyInString: the request names a ${NAME} reference
    expect(asked).toContain("`${NAME}`");
    expect(asked).toContain("`harness.mcp_env`");
    expect(asked).not.toContain("What the person said");
  });
});

describe("the other requests to the Moebius agent", () => {
  it("updates a built-in skill from its source at the commit it was copied at", () => {
    const asked = updateRequest("tdd", { url: "https://github.com/acme/skills", commit: "abc123" });
    expect(asked).toContain("git clone --depth 1 https://github.com/acme/skills import");
    expect(asked).toContain("import only the skill tdd");
    expect(asked).toContain("The copy was made at commit abc123");
  });

  it("asks for the instructions of an agent with what the person wants", () => {
    const asked = instructionsRequest("Reviewer", "Review kindly.");
    expect(asked).toContain('`"$MOEBIUS" agent show Reviewer`');
    expect(asked).toContain("--instructions <file>");
    expect(asked.endsWith("What the person wants Reviewer to do:\n\nReview kindly.")).toBe(true);
  });
});
