// @vitest-environment jsdom
/**
 * The package sheet closed and open, and its search: closed until it is opened, the place it shares with the other sheet, the results with their filter, and a registry that cannot be read.
 */
import { fireEvent, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { answers, inSheet, mount, openRegistries, stubServer, summary } from "./sheetTestKit";

stubServer();

describe("opening and closing the sheet", () => {
  it("is not on the page until it is opened, leaves no button when closed, and keeps its state", async () => {
    answers["GET /v1/packages/search?q="] = { body: { packages: [summary()], problems: [] } };
    mount();
    expect(screen.queryByRole("dialog", { name: "Package sheet" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Open the package sheet" })).toBeNull();

    openRegistries();
    await inSheet().findByText("gate");

    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "Package sheet" })).toBeNull();
    // No round button floats over the page: the sheet is opened again from where it was opened.
    expect(screen.queryByRole("button", { name: "Open the package sheet" })).toBeNull();
    openRegistries();
    // What was found is where it was left.
    expect(await inSheet().findByText("gate")).toBeTruthy();
  });

  it("puts the focus on the search field when it opens", async () => {
    answers["GET /v1/packages/search?q="] = { body: { packages: [summary()], problems: [] } };
    mount();
    openRegistries();
    const field = await inSheet().findByRole("textbox", { name: "Search the registries" });
    expect(document.activeElement).toBe(field);
  });

  it("shares its place with the other sheet: opening one hides the other, which keeps its state", async () => {
    answers["GET /v1/packages/search?q="] = { body: { packages: [summary()], problems: [] } };
    mount();
    openRegistries();
    await inSheet().findByText("gate");
    fireEvent.click(screen.getByRole("button", { name: "open the other sheet" }));
    expect(screen.queryByRole("dialog", { name: "Package sheet" })).toBeNull();
    expect(screen.getByRole("dialog", { name: "Other sheet" })).toBeTruthy();
    // Escape does not reach the hidden one.
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.getByRole("dialog", { name: "Other sheet" })).toBeTruthy();
    expect(screen.queryByRole("dialog", { name: "Package sheet" })).toBeNull();
    openRegistries();
    expect(await inSheet().findByText("gate")).toBeTruthy();
    expect(screen.queryByRole("dialog", { name: "Other sheet" })).toBeNull();
  });

  it("switches its catalog when it is opened from elsewhere", async () => {
    answers["GET /v1/packages/search?q="] = { body: { packages: [summary()], problems: [] } };
    answers["GET /v1/extensions/search?q="] = {
      body: [
        {
          name: "pi-web-access",
          version: "1.4.0",
          description: "Lets an agent fetch web pages.",
          kinds: ["extension"],
          weekly_downloads: 12400,
          npm: null,
          homepage: null,
          repository: null,
          date: null,
        },
      ],
    };
    mount();
    openRegistries();
    await inSheet().findByText("Holds cards marked wip.", { exact: false });
    fireEvent.click(screen.getByRole("button", { name: "open pi for Coder" }));
    expect(await inSheet().findByText("pi-web-access")).toBeTruthy();
    expect(inSheet().queryByText("gate")).toBeNull();
    expect(inSheet().getByText("Pi packages from pi.dev, for Coder")).toBeTruthy();
  });
});

describe("searching the registries", () => {
  it("lists what was found with its kind, version and whether it is installed or has an update", async () => {
    answers["GET /v1/packages/search?q="] = {
      body: {
        packages: [
          summary(),
          summary({ name: "review-flow", kind: "workflow", version: "1.0.0", installed: "1.0.0" }),
          summary({ name: "guard-pack", version: "2.1.0", installed: "2.0.0" }),
        ],
        problems: [],
      },
    };
    mount();
    openRegistries();
    const list = await inSheet().findByRole("list", { name: "Packages" });
    const rows = within(list).getAllByTestId("list-card");
    expect(rows).toHaveLength(3);
    expect(rows[0]?.textContent).toContain("Plugin");
    expect(rows[0]?.textContent).toContain("1.1.0");
    expect(rows[1]?.textContent).toContain("Installed 1.0.0");
    expect(rows[2]?.textContent).toContain("Update to 2.1.0");
  });

  it("narrows by kind and says when nothing is left", async () => {
    answers["GET /v1/packages/search?q="] = { body: { packages: [summary()], problems: [] } };
    mount();
    openRegistries();
    await inSheet().findByText("gate");
    fireEvent.click(inSheet().getByRole("button", { name: "Workflow" }));
    expect(await inSheet().findByText("No package found")).toBeTruthy();
    fireEvent.click(inSheet().getByRole("button", { name: "Plugin" }));
    expect(await inSheet().findByText("gate")).toBeTruthy();
  });

  it("searches as the typing pauses", async () => {
    answers["GET /v1/packages/search?q="] = { body: { packages: [summary()], problems: [] } };
    answers["GET /v1/packages/search?q=review"] = {
      body: {
        packages: [summary({ name: "review-flow", kind: "workflow" })],
        problems: [],
      },
    };
    mount();
    openRegistries();
    await inSheet().findByText("gate");
    fireEvent.change(inSheet().getByLabelText("Search the registries"), {
      target: { value: "review" },
    });
    expect(await inSheet().findByText("review-flow")).toBeTruthy();
  });

  it("says a registry that cannot be read beside the results of the others", async () => {
    answers["GET /v1/packages/search?q="] = {
      body: {
        packages: [summary()],
        problems: [{ registry: "acme", message: "cannot read index.toml: not valid" }],
      },
    };
    mount();
    openRegistries();
    const notice = await inSheet().findByRole("alert");
    expect(notice.textContent).toContain("The registry acme could not be read.");
    expect(notice.textContent).toContain("cannot read index.toml");
    expect(inSheet().getByText("gate")).toBeTruthy();
  });

  it("shows a search that failed as a notice in the sheet, with a way to try again", async () => {
    answers["GET /v1/packages/search?q="] = {
      status: 502,
      body: "the registry could not be reached",
    };
    mount();
    openRegistries();
    const notice = await inSheet().findByRole("alert");
    expect(notice.textContent).toContain("the registry could not be reached");
    expect(inSheet().getByRole("button", { name: "Try again" })).toBeTruthy();
  });
});
