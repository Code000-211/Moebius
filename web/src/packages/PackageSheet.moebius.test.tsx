// @vitest-environment jsdom
/**
 * The package sheet on a Moebius package: its versions, an install told in the API's words (the plugin's error, the restart, a change a person merges, a refusal), and update and remove.
 */
import { fireEvent, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
  answers,
  info,
  inSheet,
  mount,
  openRegistries,
  requests,
  stubServer,
  summary,
} from "./sheetTestKit";

stubServer();

describe("a Moebius package", () => {
  async function openGate(kind = "plugin") {
    answers["GET /v1/packages/search?q="] = {
      body: { packages: [summary({ kind })], problems: [] },
    };
    answers["GET /v1/packages/gate"] = { body: info(kind) };
    mount();
    openRegistries();
    fireEvent.click(await inSheet().findByText("gate"));
    await inSheet().findByTestId("package-detail");
  }

  it("shows its versions with whether each works with this Moebius", async () => {
    await openGate();
    const versions = inSheet().getByRole("list", { name: "Versions" });
    expect(versions.textContent).toContain("1.1.0");
    expect(versions.textContent).toContain("Works with this Moebius");
    expect(versions.textContent).toContain("Needs Moebius >=9.0");
  });

  it("installs a plugin at once and says what it did, as the API gives it", async () => {
    answers["POST /v1/packages"] = {
      body: {
        name: "gate",
        version: "1.1.0",
        kind: "plugin",
        action: "installed",
        restart_needed: true,
        plugin: { program: "/p/gate", state: "running", restarts: 0, error: null },
      },
    };
    await openGate();
    fireEvent.click(inSheet().getByRole("button", { name: "Install" }));
    const report = await inSheet().findByTestId("package-report");
    expect(report.textContent).toContain("Installed gate 1.1.0.");
    expect(report.textContent).toContain("Restart the server for it to take effect.");
    expect(requests.find((each) => each.method === "POST")?.body).toEqual({
      name: "gate",
      registry: "moebius-plugins",
    });
  });

  it("does nothing for a second press while a request is running", async () => {
    let release: () => void = () => {};
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const base = globalThis.fetch;
    answers["POST /v1/packages"] = {
      body: {
        name: "gate",
        version: "1.1.0",
        kind: "plugin",
        action: "installed",
        restart_needed: false,
      },
    };
    await openGate();
    vi.stubGlobal("fetch", async (path: string, init: RequestInit) => {
      if (init.method === "POST") await held;
      return base(path, init);
    });
    const install = inSheet().getByRole("button", { name: "Install" });
    fireEvent.click(install);
    fireEvent.click(install);
    release();
    await inSheet().findByTestId("package-report");
    expect(requests.filter((each) => each.method === "POST")).toHaveLength(1);
  });

  it("shows the error of a plugin that does not run", async () => {
    answers["POST /v1/packages"] = {
      body: {
        name: "gate",
        version: "1.1.0",
        kind: "plugin",
        action: "installed",
        restart_needed: false,
        plugin: {
          program: "/p/gate",
          state: "restarting",
          restarts: 1,
          error: "exited with status 1",
        },
      },
    };
    await openGate();
    fireEvent.click(inSheet().getByRole("button", { name: "Install" }));
    const alerts = await inSheet().findAllByRole("alert");
    expect(alerts[0]?.textContent).toContain("gate does not run: exited with status 1");
  });

  it("says a workflow is a change a person merges, and links to it in the Inbox", async () => {
    answers["POST /v1/packages"] = {
      body: {
        name: "gate",
        version: "1.1.0",
        kind: "workflow",
        action: "installed",
        restart_needed: false,
        change: 12,
      },
    };
    await openGate("workflow");
    expect(inSheet().getByText(/Nothing is written until a person merges it/)).toBeTruthy();
    fireEvent.click(inSheet().getByRole("button", { name: "Propose install" }));
    const link = await inSheet().findByRole("link", { name: "See it in the Inbox" });
    expect(link.getAttribute("href")).toBe("/inbox?change=12");
    fireEvent.click(link);
    expect(screen.getByTestId("where").textContent).toBe("/inbox?change=12");
    // The sheet makes room for the Inbox.
    expect(screen.queryByRole("dialog", { name: "Package sheet" })).toBeNull();
  });

  it("shows a refusal with its sentence", async () => {
    answers["POST /v1/packages"] = {
      status: 422,
      body: "gate 1.1.0 works with Moebius >=9.0, and this is Moebius 0.1.0",
    };
    await openGate();
    fireEvent.click(inSheet().getByRole("button", { name: "Install" }));
    const alert = await inSheet().findByRole("alert");
    expect(alert.textContent).toContain("gate 1.1.0 works with Moebius >=9.0");
    // The package is still open, and can be tried again.
    expect(inSheet().getByRole("button", { name: "Install" })).toBeTruthy();
  });

  it("says a session that is not a person's cannot do it", async () => {
    answers["POST /v1/packages"] = { status: 403, body: "only a person can install a plugin" };
    await openGate();
    fireEvent.click(inSheet().getByRole("button", { name: "Install" }));
    const alert = await inSheet().findByRole("alert");
    expect(alert.textContent).toContain("this session is not a person's");
  });

  it("updates and removes an installed one, asking before it removes", async () => {
    answers["GET /v1/packages/search?q="] = {
      body: { packages: [summary({ installed: "1.0.0" })], problems: [] },
    };
    answers["GET /v1/packages/gate"] = {
      body: info("plugin", {
        name: "gate",
        version: "1.0.0",
        kind: "plugin",
        registry: "moebius-plugins",
      }),
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
    answers["DELETE /v1/packages/gate"] = {
      body: {
        name: "gate",
        version: "1.1.0",
        kind: "plugin",
        action: "removed",
        restart_needed: false,
      },
    };
    mount();
    openRegistries();
    fireEvent.click(await inSheet().findByText("gate"));
    fireEvent.click(await inSheet().findByRole("button", { name: "Update to 1.1.0" }));
    expect((await inSheet().findByTestId("package-report")).textContent).toContain(
      "Updated gate to 1.1.0.",
    );
    fireEvent.click(inSheet().getByRole("button", { name: "Remove gate" }));
    // Nothing is removed by the first press.
    expect(requests.some((each) => each.method === "DELETE")).toBe(false);
    fireEvent.click(inSheet().getByRole("button", { name: "Keep it" }));
    expect(requests.some((each) => each.method === "DELETE")).toBe(false);
    fireEvent.click(inSheet().getByRole("button", { name: "Remove gate" }));
    fireEvent.click(inSheet().getByRole("button", { name: "Confirm removing gate" }));
    expect((await inSheet().findByTestId("package-report")).textContent).toContain("Removed gate.");
  });
});
