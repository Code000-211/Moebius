// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import { PeoplePage } from "./PeoplePage";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const person = (id: number, handle: string) => ({
  id,
  handle,
  name: handle,
  role: "owner",
  disabled: false,
  created_at: 1,
});

/** A server with two owners, `lea` (id 1) being the one signed in; the changes of a person are recorded. */
function show() {
  const sent: string[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: { method?: string; body?: string }) => {
      const reply = (body: unknown) => ({
        ok: true,
        status: 200,
        text: async () => JSON.stringify(body),
      });
      if (init?.method === "PATCH") {
        sent.push(`${url} ${init.body}`);
        return reply({ ...person(1, "lea"), disabled: true });
      }
      return reply([person(1, "lea"), person(2, "tom")]);
    }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(keys.session, { signed_in: true, person: person(1, "lea") });
  render(
    <QueryClientProvider client={client}>
      <PeoplePage />
    </QueryClientProvider>,
  );
  return sent;
}

describe("disabling a person", () => {
  it("asks before an owner disables their own row, and changes nothing until they say so", async () => {
    const sent = show();
    await screen.findByTestId("person-lea");
    fireEvent.click(screen.getAllByRole("button", { name: "Disable" })[0] as HTMLElement);
    await screen.findByText("Disable yourself?");
    expect(sent).toEqual([]);
    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(sent).toEqual([]);
  });

  it("disables yourself once confirmed", async () => {
    const sent = show();
    await screen.findByTestId("person-lea");
    fireEvent.click(screen.getAllByRole("button", { name: "Disable" })[0] as HTMLElement);
    fireEvent.click(await screen.findByRole("button", { name: "Disable me" }));
    await waitFor(() => expect(sent).toEqual(['/v1/people/1 {"disabled":true}']));
  });

  it("disables another person with one click", async () => {
    const sent = show();
    await screen.findByTestId("person-tom");
    fireEvent.click(screen.getAllByRole("button", { name: "Disable" })[1] as HTMLElement);
    await waitFor(() => expect(sent).toEqual(['/v1/people/2 {"disabled":true}']));
  });
});
