/**
 * The rail of the desktop app draws the app's servers: the open one marked, the Inbox's count, a grey dot for one that
 * cannot be reached; a click opens one, "+" connects one, and removing one goes through the app.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { NO_SERVER, type RailServer } from "./bridge";
import { fakeBridge } from "./fakeBridge";
import { initial, Rail, SHOWN_EVENT } from "./Rail";

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
      <Rail bridge={bridge} />
    </QueryClientProvider>,
  );
}

describe("Rail", () => {
  it("draws nothing in a browser", () => {
    const client = new QueryClient();
    render(
      <QueryClientProvider client={client}>
        <Rail bridge={undefined} />
      </QueryClientProvider>,
    );
    expect(screen.queryByTestId("rail")).toBeNull();
  });

  it("tells squares apart: words by their first letters, addresses by the end of their port", () => {
    expect(initial("https://moebius.example")).toBe("ME");
    expect(initial("devbox")).toBe("DE");
    expect(initial("Build box")).toBe("BB");
    expect(initial("::")).toBe("?");
    // Three servers on one host are not three squares showing the same character.
    const three = ["127.0.0.1:7171", "127.0.0.1:7172", "127.0.0.1:7180"].map(initial);
    expect(new Set(three).size).toBe(3);
    expect(initial("laptop · 127.0.0.1:7172")).toBe("172");
    expect(initial("laptop · 127.0.0.1:7171")).not.toBe(initial("laptop · 127.0.0.1:7172"));
  });

  it("marks the open server, counts its Inbox and greys the one it cannot reach", async () => {
    draw({ ...fakeBridge(), servers: async () => SERVERS });
    const open = await screen.findByRole("button", {
      name: /127\.0\.0\.1:7171, 2 items and 1 notice waiting in the Inbox/,
    });
    expect(open.getAttribute("aria-current")).toBe("true");
    // The open server carries a mark of its own, which the other does not.
    expect(open.querySelector("[data-testid=rail-open]")).not.toBeNull();
    expect(screen.getAllByTestId("rail-open")).toHaveLength(1);
    expect(screen.getByTestId("rail-count").textContent).toBe("3");
    const grey = screen.getByRole("button", { name: "devbox (cannot be reached)" });
    expect(grey.querySelector("[data-testid=rail-unreachable]")).not.toBeNull();
    expect(open.querySelector("[data-testid=rail-unreachable]")).toBeNull();
  });

  it("opens another server with a click", async () => {
    const switchServer = vi.fn(async () => {});
    draw({ ...fakeBridge(), servers: async () => SERVERS, switchServer });
    fireEvent.click(await screen.findByRole("button", { name: /devbox/ }));
    expect(switchServer).toHaveBeenCalledWith("b");
  });

  it("marks the server asked for at once, before the app has shown it, and reads the servers when it has", async () => {
    let finish = () => {};
    const switchServer = vi.fn(() => new Promise<void>((resolve) => (finish = resolve)));
    const servers = vi.fn(async () => SERVERS);
    draw({ ...fakeBridge(), servers, switchServer });
    const other = await screen.findByRole("button", { name: /devbox/ });
    fireEvent.click(other);
    expect(other.getAttribute("aria-current")).toBe("true");
    expect(other.getAttribute("aria-busy")).toBe("true");
    expect(
      screen.getByRole("button", { name: /127\.0\.0\.1:7171/ }).getAttribute("aria-current"),
    ).toBeNull();
    const reads = servers.mock.calls.length;
    finish();
    await waitFor(() => expect(servers.mock.calls.length).toBeGreaterThan(reads));
    await waitFor(() => expect(other.getAttribute("aria-busy")).toBeNull());
  });

  it("reads the servers again when the app shows this page after another server's", async () => {
    const servers = vi.fn(async () => SERVERS);
    draw({ ...fakeBridge(), servers });
    await screen.findByRole("button", { name: /devbox/ });
    const reads = servers.mock.calls.length;
    window.dispatchEvent(new Event(SHOWN_EVENT));
    await waitFor(() => expect(servers.mock.calls.length).toBeGreaterThan(reads));
  });

  it("removes a server from its menu", async () => {
    const removeServer = vi.fn(async () => {});
    draw({ ...fakeBridge(), servers: async () => SERVERS, removeServer });
    fireEvent.contextMenu(await screen.findByRole("button", { name: /devbox/ }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Remove server" }));
    await waitFor(() => expect(removeServer).toHaveBeenCalledWith("b"));
  });

  it("renames a server from its menu, and an empty name gives its own back", async () => {
    const renameServer = vi.fn(async () => {});
    draw({ ...fakeBridge(), servers: async () => SERVERS, renameServer });
    fireEvent.contextMenu(await screen.findByRole("button", { name: /devbox/ }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Rename server" }));
    const field = await screen.findByLabelText("Name");
    expect((field as HTMLInputElement).value).toBe("devbox");
    fireEvent.change(field, { target: { value: "Build box" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(renameServer).toHaveBeenCalledWith("b", "Build box"));
  });

  it("labels the fields of the dialog, as the first-start page does", async () => {
    draw({ ...fakeBridge() });
    fireEvent.click(await screen.findByRole("button", { name: "Connect a server" }));
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
    fireEvent.click(await screen.findByRole("button", { name: "Connect a server" }));
    fireEvent.change(await screen.findByLabelText("Address"), { target: { value: "h:1" } });
    fireEvent.change(screen.getByLabelText("Access token"), { target: { value: "tok" } });
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      "the server does not accept this access token",
    );
    expect(connect).toHaveBeenCalledWith("h:1", "tok");
  });

  it("sets a host up over SSH only after a yes", async () => {
    const connectSsh = vi.fn(async (_host: string, setUp: boolean) => {
      if (!setUp) throw `${NO_SERVER} no server is set up on devbox`;
    });
    draw({ ...fakeBridge(), connectSsh });
    fireEvent.click(await screen.findByRole("button", { name: "Connect a server" }));
    fireEvent.click(await screen.findByRole("button", { name: "Over SSH" }));
    fireEvent.change(screen.getByLabelText("SSH host"), { target: { value: "devbox" } });
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    fireEvent.click(await screen.findByRole("button", { name: "Yes, set it up" }));
    await waitFor(() => expect(connectSsh).toHaveBeenLastCalledWith("devbox", true));
    expect(connectSsh).toHaveBeenNthCalledWith(1, "devbox", false);
  });
});
