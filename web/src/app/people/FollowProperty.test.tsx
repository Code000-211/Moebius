// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { FollowProperty } from "./FollowProperty";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

/** A server where `lea` is signed in and the card is followed by `followers`; the follow requests are recorded. */
function server(followers: string[]) {
  const sent: Array<[string, unknown]> = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: { method?: string; body?: string }) => {
      const reply = (body: unknown) => ({
        ok: true,
        status: 200,
        text: async () => JSON.stringify(body),
      });
      if (url === "/v1/session") return reply({ signed_in: true, person: { handle: "Lea" } });
      if (init?.method === "PUT") {
        const { following } = JSON.parse(init.body ?? "{}") as { following: boolean };
        sent.push([url, following]);
        return reply(following ? ["tom", "lea"] : ["tom"]);
      }
      return reply(followers);
    }),
  );
  return sent;
}

function show() {
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <FollowProperty card={7} />
    </QueryClientProvider>,
  );
}

describe("the Following property of a card", () => {
  it("says the person follows it and stops following with one request", async () => {
    const sent = server(["tom", "lea"]);
    show();
    await screen.findByText("You follow it");
    fireEvent.click(screen.getByRole("button", { name: "Stop following" }));
    await screen.findByText("Not following");
    expect(sent).toEqual([["/v1/cards/7/follow", false]]);
  });

  it("follows a card that was not followed", async () => {
    const sent = server(["tom"]);
    show();
    fireEvent.click(await screen.findByRole("button", { name: "Follow" }));
    await waitFor(() => expect(screen.getByText("You follow it")).toBeTruthy());
    expect(sent).toEqual([["/v1/cards/7/follow", true]]);
  });
});
