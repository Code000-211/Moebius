// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ToastProvider } from "../page/Toast";
import { useErrorToast } from "./useErrorToast";

afterEach(cleanup);

function Page({ error }: { error: unknown }) {
  useErrorToast(error, "Couldn't save.");
  return null;
}

describe("useErrorToast", () => {
  it("says a failure as a toast, with the server's words after what could not be done", () => {
    render(
      <ToastProvider>
        <Page error={new Error("the name is taken")} />
      </ToastProvider>,
    );
    expect(screen.getByRole("alert").textContent).toContain("Couldn't save. the name is taken");
  });

  it("says nothing while there is no failure", () => {
    render(
      <ToastProvider>
        <Page error={null} />
      </ToastProvider>,
    );
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("falls back to Try again for a failure with no words", () => {
    render(
      <ToastProvider>
        <Page error={new Error("")} />
      </ToastProvider>,
    );
    expect(screen.getByRole("alert").textContent).toContain("Couldn't save. Try again.");
  });
});
