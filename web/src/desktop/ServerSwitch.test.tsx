/**
 * The server switch in the sidebar's head: in the desktop app a menu of the app's servers (the open one ticked, the
 * Inbox's count, "cannot be reached" for one that does not answer) where one is opened, connected, renamed or
 * removed; in a browser only the server's name, with no menu.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { NO_SERVER, type RailServer } from "./bridge";
import { fakeBridge } from "./fakeBridge";
import { initial, ServerSwitch, SHOWN_EVENT } from "./ServerSwitch";

afterEach(cleanup);

const SERVERS: RailServer[] = [
  {
    id: "a",
    name: "127.0.0.1:7171",
    address: "http://127.0.0.1:7171",
    open: true,
    reachable: true,
    inbox: 3,
    items: 2,
    unread: 1,
  },
  {
    id: "b",
    name: "devbox",
    address: "http://devbox:7171",
    open: false,
    reachable: false,
    inbox: 0,
    items: 0,
    unread: 0,
  },
];

function draw(bridge = fakeBridge()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <ServerSwitch bridge={bridge} name="Acme" />
    </QueryClientProvider>,
  );
}

/** Opens the menu the way a keyboard does, which a test environment without pointer capture supports. */
async function openMenu() {
  const trigger = await screen.findByTestId("server-switch");
  fireEvent.keyDown(trigger, { key: "Enter" });
  await screen.findByRole("menu");
}

describe("ServerSwitch", () => {
  it("shows only the name in a browser, with no menu and nothing called Servers", () => {
    const client = new QueryClient();
    render(
      <QueryClientProvider client={client}>
        <ServerSwitch bridge={undefined} name="Acme" />
      </QueryClientProvider>,
    );
    expect(screen.getByTestId("server-switch").textContent).toBe("AAcme");
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.queryByLabelText("Servers")).toBeNull();
  });

  it("marks a server by the first letter or digit of its name", () => {
    expect(initial("Acme")).toBe("A");
    expect(initial("https://moebius.example")).toBe("M");
    expect(initial("127.0.0.1:7171")).toBe("1");
    expect(initial("::")).toBe("?");
  });

  it("names the open server in the field, and no element is labelled Servers", async () => {
    draw({ ...fakeBridge(), servers: async () => SERVERS });
    await waitFor(() =>
      expect(screen.getByTestId("server-switch").textContent).toContain("127.0.0.1:7171"),
    );
    expect(screen.queryByLabelText("Servers")).toBeNull();
  });

  it("ticks the open server, counts its Inbox and says which one cannot be reached", async () => {
    draw({ ...fakeBridge(), servers: async () => SERVERS });
    await screen.findByText("127.0.0.1:7171", { selector: "[data-testid=server-switch] span" });
    await openMenu();
    const items = screen.getAllByRole("menuitem");
    expect(items[0]?.getAttribute("aria-current")).toBe("true");
    expect(screen.getByTestId("server-count").textContent).toBe("3");
    expect(screen.getByTestId("server-count").getAttribute("title")).toBe(
      "2 items and 1 notice waiting in the Inbox",
    );
    expect(items[1]?.textContent).toContain("devbox");
    expect(
      items[1]?.querySelector("[data-testid=server-unreachable]")?.getAttribute("aria-label"),
    ).toBe("cannot be reached");
    expect(items[0]?.querySelector("[data-testid=server-unreachable]")).toBeNull();
    expect(items[1]?.getAttribute("aria-current")).toBeNull();
  });

  it("opens another server from the menu", async () => {
    const switchServer = vi.fn(async () => {});
    draw({ ...fakeBridge(), servers: async () => SERVERS, switchServer });
    await screen.findByText("127.0.0.1:7171", { selector: "[data-testid=server-switch] span" });
    await openMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: /devbox/ }));
    expect(switchServer).toHaveBeenCalledWith("b");
  });

  it("marks the server asked for at once, before the app has shown it, and reads the servers when it has", async () => {
    let finish = () => {};
    const switchServer = vi.fn(() => new Promise<void>((resolve) => (finish = resolve)));
    const servers = vi.fn(async () => SERVERS);
    draw({ ...fakeBridge(), servers, switchServer });
    await screen.findByText("127.0.0.1:7171", { selector: "[data-testid=server-switch] span" });
    await openMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: /devbox/ }));
    const field = screen.getByTestId("server-switch");
    await waitFor(() => expect(field.textContent).toContain("devbox"));
    expect(field.getAttribute("aria-busy")).toBe("true");
    const reads = servers.mock.calls.length;
    finish();
    await waitFor(() => expect(servers.mock.calls.length).toBeGreaterThan(reads));
    await waitFor(() => expect(field.getAttribute("aria-busy")).toBeNull());
  });

  it("reads the servers again when the app shows this page after another server's", async () => {
    const servers = vi.fn(async () => SERVERS);
    draw({ ...fakeBridge(), servers });
    await waitFor(() => expect(servers).toHaveBeenCalled());
    const reads = servers.mock.calls.length;
    window.dispatchEvent(new Event(SHOWN_EVENT));
    await waitFor(() => expect(servers.mock.calls.length).toBeGreaterThan(reads));
  });

  it("removes a server from the menu", async () => {
    const removeServer = vi.fn(async () => {});
    draw({ ...fakeBridge(), servers: async () => SERVERS, removeServer });
    await screen.findByText("127.0.0.1:7171", { selector: "[data-testid=server-switch] span" });
    await openMenu();
    const sub = screen.getByRole("menuitem", { name: "Remove server" });
    fireEvent.keyDown(sub, { key: "ArrowRight" });
    const rows = await screen.findAllByRole("menuitem", { name: "devbox" });
    fireEvent.click(rows[rows.length - 1] as HTMLElement);
    await waitFor(() => expect(removeServer).toHaveBeenCalledWith("b"));
  });

  it("renames a server from the menu, and an empty name gives its own back", async () => {
    const renameServer = vi.fn(async () => {});
    draw({ ...fakeBridge(), servers: async () => SERVERS, renameServer });
    await screen.findByText("127.0.0.1:7171", { selector: "[data-testid=server-switch] span" });
    await openMenu();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Rename server" }), {
      key: "ArrowRight",
    });
    const rows = await screen.findAllByRole("menuitem", { name: "devbox" });
    fireEvent.click(rows[rows.length - 1] as HTMLElement);
    const field = await screen.findByLabelText("Name");
    expect((field as HTMLInputElement).value).toBe("devbox");
    fireEvent.change(field, { target: { value: "Build box" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(renameServer).toHaveBeenCalledWith("b", "Build box"));
  });

  it("labels the fields of the dialog, as the first-start page does", async () => {
    draw({ ...fakeBridge() });
    await openMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: "Connect a server" }));
    expect(await screen.findByText("Address", { selector: "label span" })).toBeTruthy();
    expect(screen.getByText("Access token", { selector: "label span" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Over SSH" }));
    expect(screen.getByText("SSH host", { selector: "label span" })).toBeTruthy();
  });

  it("connects a server by its address and token, and says why one is refused", async () => {
    const connect = vi.fn(async () => {
      throw "the server does not accept this access token";
    });
    draw({ ...fakeBridge(), connect });
    await openMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: "Connect a server" }));
    fireEvent.change(await screen.findByLabelText("Address"), { target: { value: "h:1" } });
    fireEvent.change(screen.getByLabelText("Access token"), { target: { value: "tok" } });
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      "the server does not accept this access token",
    );
    expect(connect).toHaveBeenCalledWith("h:1", "tok", false);
    fireEvent.click(screen.getByLabelText("Stay signed in"));
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    await waitFor(() => expect(connect).toHaveBeenLastCalledWith("h:1", "tok", true));
  });

  it("sets a host up over SSH only after a yes", async () => {
    const connectSsh = vi.fn(async (_host: string, setUp: boolean) => {
      if (!setUp) throw `${NO_SERVER} no server is set up on devbox`;
    });
    draw({ ...fakeBridge(), connectSsh });
    await openMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: "Connect a server" }));
    fireEvent.click(await screen.findByRole("button", { name: "Over SSH" }));
    fireEvent.change(screen.getByLabelText("SSH host"), { target: { value: "devbox" } });
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    fireEvent.click(await screen.findByRole("button", { name: "Yes, set it up" }));
    await waitFor(() => expect(connectSsh).toHaveBeenLastCalledWith("devbox", true));
    expect(connectSsh).toHaveBeenNthCalledWith(1, "devbox", false);
  });
});
