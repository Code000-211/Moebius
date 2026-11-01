import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../api/http";
import { SignIn, signInMessage } from "./SignIn";

afterEach(() => vi.unstubAllGlobals());

function answers(status: number, body = "") {
  const fake = vi.fn().mockResolvedValue(new Response(status === 204 ? null : body, { status }));
  vi.stubGlobal("fetch", fake);
  return fake;
}

describe("signInMessage", () => {
  it("explains each way a sign-in fails", () => {
    expect(signInMessage(new ApiError(401, "x"))).toBe("That token is not accepted.");
    expect(signInMessage(new ApiError(403, "only a person can do this"))).toContain("run token");
    expect(signInMessage(new ApiError(0, "x"))).toBe("The server could not be reached.");
    expect(signInMessage(new ApiError(500, "git failed"))).toBe("git failed");
    expect(signInMessage(new Error("boom"))).toBe("Signing in failed.");
  });
});

describe("SignIn", () => {
  it("posts the token and tells the app it is signed in", async () => {
    const fake = answers(204);
    const onSignedIn = vi.fn();
    render(<SignIn onSignedIn={onSignedIn} />);
    fireEvent.change(screen.getByLabelText("Token"), { target: { value: "  secret  " } });
    fireEvent.click(screen.getByRole("button", { name: "Sign in" }));
    await waitFor(() => expect(onSignedIn).toHaveBeenCalledOnce());
    const [path, init] = fake.mock.calls[0] as [string, RequestInit];
    expect(path).toBe("/v1/session");
    expect(init.method).toBe("POST");
    expect(init.body).toBe('{"token":"secret"}');
  });

  it("shows the refusal and stays", async () => {
    answers(401, "a valid token is needed");
    const onSignedIn = vi.fn();
    render(<SignIn onSignedIn={onSignedIn} />);
    fireEvent.change(screen.getByLabelText("Token"), { target: { value: "wrong" } });
    fireEvent.click(screen.getByRole("button", { name: "Sign in" }));
    expect((await screen.findByRole("alert")).textContent).toBe("That token is not accepted.");
    expect(onSignedIn).not.toHaveBeenCalled();
  });

  it("does not send an empty token", () => {
    const fake = answers(204);
    render(<SignIn onSignedIn={() => {}} />);
    expect((screen.getByRole("button", { name: "Sign in" }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    expect(fake).not.toHaveBeenCalled();
  });
});
