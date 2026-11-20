// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "./Dialog";

afterEach(cleanup);

function Opened() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Open it
      </button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogTitle>Sheet</DialogTitle>
          <DialogDescription>Something to say</DialogDescription>
          <input aria-label="Field" />
        </DialogContent>
      </Dialog>
    </>
  );
}

describe("a dialog", () => {
  it("hands the focus back to the element that had it when it opened, after Escape", async () => {
    render(<Opened />);
    const opener = screen.getByRole("button", { name: "Open it" });
    opener.focus();
    fireEvent.click(opener);
    const field = await screen.findByLabelText("Field");
    field.focus();
    fireEvent.keyDown(field, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(opener));
  });
});
