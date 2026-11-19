// @vitest-environment jsdom
/**
 * The package sheet on the Pi catalog: weekly downloads, what a package holds, and a proposal for the agent it was opened for.
 */
import { fireEvent, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { answers, inSheet, mount, requests, stubServer } from "./sheetTestKit";

stubServer();

describe("a Pi package", () => {
  const hit = {
    name: "pi-web-access",
    version: "1.4.0",
    description: "Lets an agent fetch web pages.",
    kinds: ["extension"],
    weekly_downloads: 12400,
    npm: "https://www.npmjs.com/package/pi-web-access",
    homepage: null,
    repository: null,
    date: null,
  };
  const piInfo = {
    name: "pi-web-access",
    source: "npm:pi-web-access",
    version: "1.4.0",
    resolved: "",
    integrity: null,
    description: "Lets an agent fetch web pages.",
    resources: { extensions: ["extensions/web.ts"], skills: [], prompts: [], themes: [] },
    dependencies: ["undici"],
    install_scripts: ["postinstall"],
    files: 7,
    installed: null,
  };

  async function openHit() {
    answers["GET /v1/extensions/search?q="] = { body: [hit] };
    answers["GET /v1/extensions/packages"] = { body: [] };
    answers["GET /v1/extensions/info?source=npm%3Api-web-access"] = { body: piInfo };
    mount();
    fireEvent.click(screen.getByRole("button", { name: "open pi for Coder" }));
    fireEvent.click(await inSheet().findByText("pi-web-access"));
    await inSheet().findByTestId("package-detail");
  }

  it("shows weekly downloads in the results and what the package holds when opened", async () => {
    answers["GET /v1/extensions/search?q="] = { body: [hit] };
    answers["GET /v1/extensions/packages"] = { body: [] };
    answers["GET /v1/extensions/info?source=npm%3Api-web-access"] = { body: piInfo };
    mount();
    fireEvent.click(screen.getByRole("button", { name: "open pi for Coder" }));
    const row = await inSheet().findByTestId("list-card");
    expect(row.textContent).toContain("12.4k a week");
    fireEvent.click(row);
    const detail = await inSheet().findByTestId("package-detail");
    expect(detail.textContent).toContain("1 extension: extensions/web.ts");
    expect(detail.textContent).toContain(
      "npm would run postinstall on an install. Moebius never does.",
    );
  });

  it("proposes the install with the agent it was opened for, and links to the change", async () => {
    answers["POST /v1/extensions/packages"] = {
      body: {
        change: { id: 41 },
        package: "pi-web-access",
        action: "add",
        source: "npm:pi-web-access",
        from: null,
        to: "1.4.0",
        resources: { extensions: [], skills: [], prompts: [], themes: [] },
        agents: ["Coder"],
        changelog: "",
        files: [],
        dependency_files: 0,
        diff: "",
      },
    };
    await openHit();
    expect(inSheet().getByText(/names the package for Coder/)).toBeTruthy();
    fireEvent.click(inSheet().getByRole("button", { name: "Propose install for Coder" }));
    const link = await inSheet().findByRole("link", { name: "See it in the Inbox" });
    expect(link.getAttribute("href")).toBe("/inbox?change=41");
    expect(requests.find((each) => each.method === "POST")?.body).toEqual({
      source: "npm:pi-web-access",
      agent: "Coder",
    });
  });

  it("rests Propose after a proposal, so there is no second change", async () => {
    answers["POST /v1/extensions/packages"] = {
      body: {
        change: { id: 41 },
        package: "pi-web-access",
        action: "add",
        source: "npm:pi-web-access",
        from: null,
        to: "1.4.0",
        resources: { extensions: [], skills: [], prompts: [], themes: [] },
        agents: ["Coder"],
        changelog: "",
        files: [],
        dependency_files: 0,
        diff: "",
      },
    };
    await openHit();
    const propose = inSheet().getByRole("button", { name: "Propose install for Coder" });
    fireEvent.click(propose);
    fireEvent.click(propose);
    await inSheet().findByRole("link", { name: "See it in the Inbox" });
    fireEvent.click(inSheet().getByRole("button", { name: "Propose install for Coder" }));
    expect(requests.filter((each) => each.method === "POST")).toHaveLength(1);
  });

  it("reads and proposes an installed package from the source it was added from", async () => {
    const source = "git:github.com/acme/pi-notes";
    answers["GET /v1/extensions/search?q="] = { body: [] };
    answers["GET /v1/extensions/packages"] = {
      body: [
        {
          name: "pi-notes",
          source,
          version: "0.3.0",
          resolved: "",
          integrity: null,
          sha256: null,
          resources: { extensions: [], skills: [], prompts: [], themes: [] },
          install_scripts: [],
          agents: [],
          error: null,
        },
      ],
    };
    answers[`GET /v1/extensions/info?source=${encodeURIComponent(source)}`] = {
      body: { ...piInfo, name: "pi-notes", source, installed: "0.3.0" },
    };
    mount();
    fireEvent.click(screen.getByRole("button", { name: "open pi for Coder" }));
    fireEvent.click(await inSheet().findByRole("tab", { name: "Installed" }));
    fireEvent.click(await inSheet().findByRole("button", { name: "Details of pi-notes" }));
    await inSheet().findByTestId("package-detail");
    expect(requests.some((each) => each.path.includes("info?source=npm"))).toBe(false);
    expect(requests.some((each) => each.path.includes(encodeURIComponent(source)))).toBe(true);
  });

  it("shows a refusal with its sentence", async () => {
    answers["POST /v1/extensions/packages"] = {
      status: 409,
      body: "pi-web-access is installed already",
    };
    await openHit();
    fireEvent.click(inSheet().getByRole("button", { name: "Propose install for Coder" }));
    expect((await inSheet().findByRole("alert")).textContent).toContain("is installed already");
  });

  it("updates every installed package and shows the changelog of each", async () => {
    answers["GET /v1/extensions/search?q="] = { body: [] };
    answers["GET /v1/extensions/packages"] = {
      body: [
        {
          name: "pi-notes",
          source: "npm:pi-notes",
          version: "0.3.0",
          resolved: "",
          integrity: null,
          sha256: null,
          resources: { extensions: [], skills: [], prompts: [], themes: [] },
          install_scripts: [],
          agents: ["Coder"],
          error: null,
        },
      ],
    };
    answers["POST /v1/extensions/update"] = {
      body: {
        offered: [
          {
            change: { id: 42 },
            package: "pi-notes",
            action: "update",
            source: "npm:pi-notes",
            from: "0.3.0",
            to: "0.4.0",
            resources: { extensions: [], skills: [], prompts: [], themes: [] },
            agents: [],
            changelog: "## 0.4.0\n\n- Notes survive a restart.\n",
            files: [],
            dependency_files: 0,
            diff: "",
          },
        ],
        up_to_date: [],
        failed: [{ package: "pi-broken", error: "the registry answered 503" }],
      },
    };
    mount();
    fireEvent.click(screen.getByRole("button", { name: "open pi for Coder" }));
    fireEvent.click(await inSheet().findByRole("tab", { name: "Installed" }));
    await inSheet().findByTestId("installed-package");
    fireEvent.click(inSheet().getByRole("button", { name: "Update all" }));
    const changelog = await inSheet().findByRole("region", { name: "Changelog of pi-notes" });
    expect(changelog.textContent).toContain("Notes survive a restart.");
    const report = inSheet().getByTestId("package-report");
    expect(report.textContent).toContain("Proposed updating pi-notes from 0.3.0 to 0.4.0");
    expect(report.textContent).toContain(
      "pi-broken could not be updated. the registry answered 503",
    );
  });
});
