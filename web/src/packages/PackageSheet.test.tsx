// @vitest-environment jsdom
/**
 * The package sheet's states against a stubbed server: closed until it is opened, a search with its results, its
 * filter and a registry that cannot be read, a package opened, an install told in the API's words (the plugin's error,
 * the restart, a change a person merges, a refusal), the Installed tab, and the Pi catalog with the agent it was opened
 * for. Every wait is on what the sheet shows.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SheetSlotProvider, useSheetSlot } from "../ui/page/SheetSlot";
import { PackageSheet } from "./PackageSheet";
import { PackageSheetProvider, usePackageSheet } from "./PackageSheetContext";

type Answer = { status?: number; body: unknown };
let answers: Record<string, Answer | ((body: unknown) => Answer)>;
let requests: Array<{ method: string; path: string; body: unknown }>;

beforeEach(() => {
  requests = [];
  answers = {};
  vi.stubGlobal(
    "fetch",
    vi.fn(async (path: string, init: RequestInit) => {
      const method = init.method ?? "GET";
      const body = init.body ? JSON.parse(String(init.body)) : null;
      requests.push({ method, path, body });
      const given = answers[`${method} ${path}`];
      if (!given) return new Response(`no stub for ${method} ${path}`, { status: 404 });
      const answer = typeof given === "function" ? given(body) : given;
      const text = typeof answer.body === "string" ? answer.body : JSON.stringify(answer.body);
      return new Response(text, { status: answer.status ?? 200 });
    }),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const summary = (extra: Record<string, unknown> = {}) => ({
  name: "gate",
  kind: "plugin",
  version: "1.1.0",
  description: "Holds cards marked wip.",
  registry: "moebius-plugins",
  compatible: true,
  installed: null,
  ...extra,
});

const info = (kind = "plugin", installed: unknown = null) => ({
  name: "gate",
  registry: "moebius-plugins",
  installed,
  versions: [
    {
      version: "1.1.0",
      kind,
      description: "Holds cards marked wip.",
      moebius: ">=0.1",
      compatible: true,
    },
    { version: "0.9.0", kind, description: "", moebius: ">=9.0", compatible: false },
  ],
});

const snapshot = {
  boards: [],
  harness_changes: [],
  inbox: [],
};

function Where() {
  return <p data-testid="where">{useLocation().pathname + useLocation().search}</p>;
}

/** Stands for the floating chat: the other tenant of the slot. */
function OtherSheet() {
  const slot = useSheetSlot();
  return (
    <>
      <button type="button" onClick={() => slot.show("chat")}>
        open the other sheet
      </button>
      {slot.active === "chat" && <section role="dialog" aria-label="Other sheet" />}
    </>
  );
}

function Openers() {
  const sheet = usePackageSheet();
  return (
    <>
      <OtherSheet />
      <button type="button" onClick={() => sheet.open("moebius")}>
        open registries
      </button>
      <button type="button" onClick={() => sheet.open("pi", "Coder")}>
        open pi for Coder
      </button>
    </>
  );
}

function mount() {
  answers["GET /v1/snapshot"] = { body: snapshot };
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={["/plugins"]}>
        <SheetSlotProvider>
          <PackageSheetProvider>
            <Openers />
            <PackageSheet />
            <Routes>
              <Route path="*" element={<Where />} />
            </Routes>
          </PackageSheetProvider>
        </SheetSlotProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const sheet = () => screen.getByRole("dialog", { name: "Package sheet" });
const inSheet = () => within(sheet());
const openRegistries = () =>
  fireEvent.click(screen.getByRole("button", { name: "open registries" }));

describe("opening and minimising the sheet", () => {
  it("is not on the page until it is opened, and a minimised sheet leaves its button", async () => {
    answers["GET /v1/packages/search?q="] = { body: { packages: [summary()], problems: [] } };
    mount();
    expect(screen.queryByRole("dialog", { name: "Package sheet" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Open the package sheet" })).toBeNull();

    openRegistries();
    await inSheet().findByText("gate");

    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "Package sheet" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Open the package sheet" }));
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

  it("shares its slot with the other sheet: opening one minimises the other, which keeps its state", async () => {
    answers["GET /v1/packages/search?q="] = { body: { packages: [summary()], problems: [] } };
    mount();
    openRegistries();
    await inSheet().findByText("gate");
    fireEvent.click(screen.getByRole("button", { name: "open the other sheet" }));
    expect(screen.queryByRole("dialog", { name: "Package sheet" })).toBeNull();
    expect(screen.getByRole("dialog", { name: "Other sheet" })).toBeTruthy();
    // Escape does not reach the minimised one, and its button is in view for opening it again.
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.getByRole("dialog", { name: "Other sheet" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open the package sheet" }));
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
