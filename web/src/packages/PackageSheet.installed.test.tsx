// @vitest-environment jsdom
/**
 * The package sheet on what is installed: removing a package that is only proposed, and the Installed tab.
 */
import { fireEvent, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { answers, info, inSheet, mount, openRegistries, stubServer, summary } from "./sheetTestKit";

stubServer();

describe("removing a package that is only proposed", () => {
  it("does not ask 'now?' for a workflow, and says it is proposed", async () => {
    answers["GET /v1/packages/search?q="] = {
      body: { packages: [summary({ kind: "workflow", installed: "1.0.0" })], problems: [] },
    };
    answers["GET /v1/packages/gate"] = {
      body: info("workflow", {
        name: "gate",
        version: "1.0.0",
        kind: "workflow",
        registry: "moebius-plugins",
      }),
    };
    answers["DELETE /v1/packages/gate"] = {
      body: {
        name: "gate",
        version: "1.0.0",
        kind: "workflow",
        action: "removed",
        restart_needed: false,
        change: 14,
      },
    };
    mount();
    openRegistries();
    fireEvent.click(await inSheet().findByText("gate"));
    fireEvent.click(await inSheet().findByRole("button", { name: "Remove gate" }));
    expect(inSheet().queryByText("Remove gate now?")).toBeNull();
    const report = await inSheet().findByTestId("package-report");
    expect(report.textContent).toContain("Proposed removing gate.");
    expect(report.textContent).not.toContain("Removed gate.");
    expect(
      (await inSheet().findByRole("link", { name: "See it in the Inbox" })).getAttribute("href"),
    ).toBe("/inbox?change=14");
  });
});

describe("the Installed tab", () => {
  it("lists what is installed, with an update where one is waiting", async () => {
    answers["GET /v1/packages/search?q="] = { body: { packages: [], problems: [] } };
    answers["GET /v1/packages"] = {
      body: [
        {
          name: "gate",
          version: "1.0.0",
          kind: "plugin",
          registry: "moebius-plugins",
          update_available: "1.1.0",
        },
        {
          name: "guard-pack",
          version: "2.0.0",
          kind: "plugin",
          registry: "moebius-plugins",
          update_available: null,
        },
      ],
    };
    answers["POST /v1/packages/gate/update"] = {
      body: {
        name: "gate",
        version: "1.1.0",
        kind: "plugin",
        action: "updated",
        restart_needed: false,
      },
    };
    mount();
    openRegistries();
    fireEvent.click(await inSheet().findByRole("tab", { name: "Installed" }));
    const rows = await inSheet().findAllByTestId("installed-package");
    expect(rows).toHaveLength(2);
    expect(rows[0]?.textContent).toContain("Update to 1.1.0");
    expect(within(rows[1] as HTMLElement).queryByRole("button", { name: /^Update/ })).toBeNull();
    fireEvent.click(within(rows[0] as HTMLElement).getByRole("button", { name: "Update gate" }));
    expect((await inSheet().findByTestId("package-report")).textContent).toContain(
      "Updated gate to 1.1.0.",
    );
  });

  it("says when nothing is installed", async () => {
    answers["GET /v1/packages/search?q="] = { body: { packages: [], problems: [] } };
    answers["GET /v1/packages"] = { body: [] };
    mount();
    openRegistries();
    fireEvent.click(await inSheet().findByRole("tab", { name: "Installed" }));
    expect(await inSheet().findByText("Nothing installed")).toBeTruthy();
  });
});
