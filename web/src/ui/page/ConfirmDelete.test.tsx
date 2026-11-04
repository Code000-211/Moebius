// @vitest-environment jsdom
/**
 * The question before a delete: it is asked once, Keep sends nothing, Delete makes the request and closes, and a
 * refusal stays in the dialog in the server's words.
 */
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ConfirmDelete } from "./ConfirmDelete";

afterEach(cleanup);

function Harness({ onConfirm }: { onConfirm: () => Promise<void> }) {
  const [open, setOpen] = useState(true);
  return (
    <>
      <span data-testid="state">{open ? "open" : "closed"}</span>
      <ConfirmDelete
        open={open}
        onOpenChange={setOpen}
        title="Delete this chat?"
        onConfirm={onConfirm}
      >
        The chat is removed.
      </ConfirmDelete>
    </>
  );
}

describe("the question before a delete", () => {
  it("says what goes, and Keep closes it without a request", () => {
    const confirm = vi.fn().mockResolvedValue(undefined);
    render(<Harness onConfirm={confirm} />);
    expect(screen.getByRole("dialog", { name: "Delete this chat?" })).toBeTruthy();
    expect(screen.getByText("The chat is removed.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(screen.getByTestId("state").textContent).toBe("closed");
    expect(confirm).not.toHaveBeenCalled();
  });

  it("makes the request once on Delete and closes when it is done", async () => {
    const confirm = vi.fn().mockResolvedValue(undefined);
    render(<Harness onConfirm={confirm} />);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.getByTestId("state").textContent).toBe("closed"));
    expect(confirm).toHaveBeenCalledTimes(1);
  });

  it("keeps the dialog open with the server's words when the request is refused", async () => {
    const confirm = vi.fn().mockRejectedValue(new Error("chat 3 is still stopping"));
    render(<Harness onConfirm={confirm} />);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await screen.findByText("chat 3 is still stopping");
    expect(screen.getByTestId("state").textContent).toBe("open");
    expect((screen.getByRole("button", { name: "Delete" }) as HTMLButtonElement).disabled).toBe(
      false,
    );
  });
});
