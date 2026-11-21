// @vitest-environment jsdom
/** The line for a new card: what the server refuses is said as a toast, and the line stays to be tried again. */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../../api/http";
import { ToastProvider } from "../../ui/page/Toast";
import { ColumnAdd } from "./ColumnAdd";

const create = vi.fn();
vi.mock("../../api/calls/board", () => ({ createIssue: (...args: unknown[]) => create(...args) }));

afterEach(cleanup);

describe("adding a card in place", () => {
  it("says a refusal as a toast in the server's words and keeps the line open", async () => {
    create.mockRejectedValue(new ApiError(409, "the Backlog is closed"));
    const onClose = vi.fn();
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ToastProvider>
          <ColumnAdd repository={1} column="Backlog" open onClose={onClose} onCreated={vi.fn()} />
        </ToastProvider>
      </QueryClientProvider>,
    );
    const line = screen.getByLabelText("Title of the new card in Backlog");
    fireEvent.change(line, { target: { value: "Fix it" } });
    fireEvent.keyDown(line, { key: "Enter" });
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("the Backlog is closed");
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Title of the new card in Backlog")).toBeTruthy();
  });
});
