/**
 * The settings plugins ask of a repository: drawn from what each plugin declares, saved whole for one plugin, a
 * refusal said with the server's sentence, and a word when no plugin asks anything. A plugin that brings its own page
 * is drawn by it, and by the fields when it cannot be.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PluginProjectSettings } from "../../api/types/projects";
import { PluginPage } from "../plugins/PluginPage";
import { pageSource } from "../plugins/pageSource";
import { PluginSettings } from "./PluginSettings";

vi.mock("../plugins/pageSource", () => ({ pageSource: vi.fn() }));

afterEach(() => {
  vi.unstubAllGlobals();
  vi.mocked(pageSource).mockReset();
  delete document.documentElement.dataset.theme;
});

const SANDBOX: PluginProjectSettings = {
  repository: 1,
  plugins: [
    {
      plugin: "podman-sandbox",
      version: "0.2.0",
      settings: [
        {
          key: "dockerfile",
          label: "Dockerfile",
          description: "The image of the runs.",
          input: "code",
          language: "dockerfile",
          value: "FROM debian\n",
        },
        { key: "network", label: "Network", description: "", input: "line", value: "" },
      ],
      enabled: true,
      ui: false,
    },
  ],
};

/** The same plugin with its own page. */
const DRAWN: PluginProjectSettings = {
  ...SANDBOX,
  plugins: SANDBOX.plugins.map((plugin) => ({ ...plugin, ui: true })),
};

/** The frame of a plugin's page, as the test sees it from the app's side, and a way to speak as the frame. */
function frame() {
  const element = screen.getByTestId("plugin-page") as HTMLIFrameElement;
  const window = element.contentWindow as Window;
  const post = vi.spyOn(window, "postMessage");
  /** What the frame says; `from` is who the browser says sent it. */
  const say = (data: unknown, from: Window | null = window) =>
    act(() => {
      globalThis.dispatchEvent(new MessageEvent("message", { data, source: from }));
    });
  /** What the app has told the frame. */
  const told = () => post.mock.calls.map(([message]) => message as Record<string, unknown>);
  return { element, window, say, told };
}

/** The frame says it is loaded and the app has sent the page, which the frame then draws. */
async function drawn() {
  const f = frame();
  await f.say({ type: "ready" });
  await waitFor(() => expect(f.told().some((m) => m.type === "init")).toBe(true));
  await f.say({ type: "mounted" });
  return f;
}

/** The panel, the server answering the read with `settings` and each save with `answer` (a status and a body). */
function shown(settings: PluginProjectSettings, answer: (body: unknown) => Response) {
  const puts: Array<{ url: string; body: unknown }> = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      if (init?.method === "PUT") {
        const body = JSON.parse(String(init.body));
        puts.push({ url, body });
        return answer(body);
      }
      return Response.json(settings);
    }),
  );
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <PluginSettings repository={1} />
    </QueryClientProvider>,
  );
  return puts;
}

describe("the plugin settings of a repository", () => {
  it("draws each setting as the plugin declares it and saves the plugin's values whole", async () => {
    const puts = shown(SANDBOX, (body) => {
      const values = (body as { values: Record<string, string> }).values;
      const plugin = SANDBOX.plugins[0];
      return Response.json({
        ...plugin,
        settings: plugin?.settings.map((s) => ({ ...s, value: values[s.key] ?? "" })),
      });
    });
    await screen.findByRole("heading", { name: "podman-sandbox" });
    expect(screen.getByText("The image of the runs.")).toBeTruthy();
    expect(await screen.findByTestId("code-editor")).toBeTruthy();
    const save = screen.getByRole("button", { name: "Save settings" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);

    fireEvent.change(screen.getByLabelText("Network"), { target: { value: "internet" } });
    expect(save.disabled).toBe(false);
    fireEvent.click(save);
    await screen.findByText("Saved. The plugin uses it from the next run.");
    expect(puts).toEqual([
      {
        url: "/v1/repositories/1/plugin-settings/podman-sandbox",
        body: { values: { dockerfile: "FROM debian\n", network: "internet" } },
      },
    ]);
    expect((screen.getByLabelText("Network") as HTMLInputElement).value).toBe("internet");
    expect(
      (screen.getByRole("button", { name: "Save settings" }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it("shows a plugin that is turned off as off and keeps its settings box", async () => {
    shown(
      { ...SANDBOX, plugins: SANDBOX.plugins.map((plugin) => ({ ...plugin, enabled: false })) },
      () => new Response(null, { status: 500 }),
    );
    await screen.findByRole("heading", { name: "podman-sandbox" });
    expect(screen.getByText("Plugin settings · 0.2.0 · Off")).toBeTruthy();
    expect((screen.getByLabelText("Network") as HTMLInputElement).disabled).toBe(false);
  });

  it("says why the server refused and keeps what was typed", async () => {
    shown(SANDBOX, () => new Response("Network is one line", { status: 422 }));
    await screen.findByRole("heading", { name: "podman-sandbox" });
    fireEvent.change(screen.getByLabelText("Network"), { target: { value: "moebius" } });
    fireEvent.click(screen.getByRole("button", { name: "Save settings" }));
    const notice = await screen.findByText("Network is one line");
    expect(notice.closest("[role=alert]")?.textContent).toContain("Couldn't save.");
    expect((screen.getByLabelText("Network") as HTMLInputElement).value).toBe("moebius");
  });

  it("draws a plugin's own page in a sandboxed frame and tells it the values, not the way to save them", async () => {
    vi.mocked(pageSource).mockResolvedValue("export default function () {}");
    document.documentElement.dataset.theme = "light";
    shown(DRAWN, () => new Response(null, { status: 500 }));
    await screen.findByTestId("plugin-page");
    const f = frame();
    // No allow-same-origin: the frame has no origin, so no cookies, storage or access to the app.
    expect(f.element.getAttribute("sandbox")).toBe("allow-scripts");
    expect(f.element.getAttribute("src")).toBe("/plugin-frame.html");
    expect(pageSource).toHaveBeenCalledWith("podman-sandbox", "0.2.0");
    // The page is sent only when the frame says it is loaded.
    expect(f.told()).toEqual([]);
    await f.say({ type: "ready" });
    await waitFor(() => expect(f.told()).toHaveLength(1));
    const init = f.told()[0] as { host: Record<string, unknown> } & Record<string, unknown>;
    expect(init).toMatchObject({
      type: "init",
      theme: "light",
      source: "export default function () {}",
      host: {
        plugin: "podman-sandbox",
        version: "0.2.0",
        scope: "project",
        repository: 1,
        values: { dockerfile: "FROM debian\n", network: "" },
      },
    });
    expect(init.host.change).toBeUndefined();
    expect(screen.queryByLabelText("Network")).toBeNull();
  });

  it("saves the values the page reports, once they are valid", async () => {
    vi.mocked(pageSource).mockResolvedValue("");
    const puts = shown(DRAWN, (body) => {
      const values = (body as { values: Record<string, string> }).values;
      const plugin = DRAWN.plugins[0];
      return Response.json({
        ...plugin,
        settings: plugin?.settings.map((s) => ({ ...s, value: values[s.key] ?? "" })),
      });
    });
    await screen.findByTestId("plugin-page");
    const f = await drawn();
    const save = screen.getByRole("button", { name: "Save settings" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);

    // Nothing the frame says beyond the declared values counts: a key the plugin does not declare, a value that is not
    // text, a line break in a one-line value, a message from a window that is not the frame, or one of an unknown kind.
    await f.say({ type: "change", values: { network: "internet", other: "x" } });
    await f.say({ type: "change", values: { network: 3 } });
    await f.say({ type: "change", values: { network: "a\nb" } });
    await f.say({ type: "change", values: ["network"] });
    await f.say({ type: "change", values: { network: "internet" } }, globalThis.window);
    await f.say({ type: "change", values: { network: "internet" } }, null);
    await f.say({ type: "save" });
    await f.say("change");
    expect(save.disabled).toBe(true);

    await f.say({ type: "change", values: { dockerfile: "FROM debian\n", network: "internet" } });
    expect(save.disabled).toBe(false);
    fireEvent.click(save);
    await screen.findByText("Saved. The plugin uses it from the next run.");
    expect(puts).toEqual([
      {
        url: "/v1/repositories/1/plugin-settings/podman-sandbox",
        body: { values: { dockerfile: "FROM debian\n", network: "internet" } },
      },
    ]);
  });

  it("sizes the frame to the height the page reports, within bounds", async () => {
    vi.mocked(pageSource).mockResolvedValue("");
    shown(DRAWN, () => new Response(null, { status: 500 }));
    await screen.findByTestId("plugin-page");
    const f = await drawn();
    await f.say({ type: "height", height: 240.4 });
    expect(f.element.style.height).toBe("241px");
    await f.say({ type: "height", height: -5 });
    await f.say({ type: "height", height: "tall" });
    expect(f.element.style.height).toBe("241px");
    await f.say({ type: "height", height: 1e9 });
    expect(f.element.style.height).toBe("20000px");
  });

  it("tells the frame when the app changes its theme", async () => {
    vi.mocked(pageSource).mockResolvedValue("");
    shown(DRAWN, () => new Response(null, { status: 500 }));
    await screen.findByTestId("plugin-page");
    const f = await drawn();
    document.documentElement.dataset.theme = "light";
    await waitFor(() =>
      expect(f.told().filter((m) => m.type === "theme")).toEqual([
        { type: "theme", theme: "light" },
      ]),
    );
  });

  it("draws the page again from the saved values on a revert", async () => {
    vi.mocked(pageSource).mockResolvedValue("");
    shown(DRAWN, () => new Response(null, { status: 500 }));
    await screen.findByTestId("plugin-page");
    const f = await drawn();
    await f.say({ type: "change", values: { network: "internet" } });
    fireEvent.click(screen.getByRole("button", { name: "Revert changes" }));
    await waitFor(() => expect(screen.getByTestId("plugin-page")).not.toBe(f.element));
    expect(
      (screen.getByRole("button", { name: "Save settings" }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it("draws the fields and says why when the plugin's page fails to draw", async () => {
    vi.mocked(pageSource).mockResolvedValue("");
    shown(DRAWN, () => new Response(null, { status: 500 }));
    await screen.findByTestId("plugin-page");
    const f = frame();
    await f.say({ type: "ready" });
    await f.say({ type: "failed", message: "the page threw" });
    await screen.findByText(/the page threw/);
    expect(screen.getByText(/Its settings are shown as fields/)).toBeTruthy();
    expect(screen.getByLabelText("Network")).toBeTruthy();
    expect(screen.queryByTestId("plugin-page")).toBeNull();
  });

  it("draws the fields when the module cannot be read", async () => {
    vi.mocked(pageSource).mockRejectedValue(new Error("no such page"));
    shown(DRAWN, () => new Response(null, { status: 500 }));
    await screen.findByText(/no such page/);
    expect(screen.getByLabelText("Network")).toBeTruthy();
  });

  it("gives up on a page that says nothing in time", async () => {
    vi.mocked(pageSource).mockResolvedValue("");
    const failed = vi.fn();
    render(
      <PluginPage
        timeoutMs={20}
        onFailed={failed}
        host={{
          plugin: "p",
          version: "1",
          scope: "app",
          repository: null,
          settings: [],
          values: {},
          change: () => {},
        }}
      />,
    );
    await waitFor(() => expect(failed).toHaveBeenCalledWith("it did not draw in time"));
  });

  it("says so when no plugin asks anything of a repository", async () => {
    shown({ repository: 1, plugins: [] }, () => new Response(null, { status: 500 }));
    await screen.findByText(/No plugin on this server asks anything of a repository/);
  });
});
