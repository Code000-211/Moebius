// @vitest-environment jsdom
/**
 * The ask before leaving the wiki editor with unsaved text: a move to another page of the wiki (only the query
 * changes) is stopped while there is text to lose and goes through when there is none, and the tab asks too.
 */
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createMemoryRouter, Link, RouterProvider } from "react-router";
import { afterEach, describe, expect, it } from "vitest";
import { LeaveGuard } from "./LeaveGuard";

afterEach(cleanup);

function open(dirty: { current: boolean }) {
  const router = createMemoryRouter(
    [
      {
        path: "/wiki",
        element: (
          <>
            <LeaveGuard dirty={() => dirty.current} />
            <Link to="/wiki?page=Other">Other page</Link>
          </>
        ),
      },
    ],
    { initialEntries: ["/wiki?page=Handbook&view=edit"] },
  );
  render(<RouterProvider router={router} />);
  return router;
}

describe("leaving the wiki editor", () => {
  it("asks first when there is unsaved text, and stays or leaves as chosen", async () => {
    const router = open({ current: true });
    fireEvent.click(screen.getByText("Other page"));
    await screen.findByText("Leave with unsaved text?");
    expect(router.state.location.search).toBe("?page=Handbook&view=edit");

    fireEvent.click(screen.getByText("Keep editing"));
    await waitFor(() => expect(screen.queryByText("Leave with unsaved text?")).toBeNull());
    expect(router.state.location.search).toBe("?page=Handbook&view=edit");

    fireEvent.click(screen.getByText("Other page"));
    fireEvent.click(await screen.findByText("Leave without saving"));
    await waitFor(() => expect(router.state.location.search).toBe("?page=Other"));
  });

  it("lets the person go when nothing is unsaved", async () => {
    const router = open({ current: false });
    fireEvent.click(screen.getByText("Other page"));
    await waitFor(() => expect(router.state.location.search).toBe("?page=Other"));
    expect(screen.queryByText("Leave with unsaved text?")).toBeNull();
  });

  it("makes the tab ask before a reload or a close, only with unsaved text", () => {
    const dirty = { current: true };
    open(dirty);
    const asked = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(asked);
    expect(asked.defaultPrevented).toBe(true);

    dirty.current = false;
    const quiet = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(quiet);
    expect(quiet.defaultPrevented).toBe(false);
  });
});
