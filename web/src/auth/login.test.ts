import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../api/http";
import { loginOutcome, startLogin, takeLoginCode } from "./login";

afterEach(() => window.history.replaceState(null, "", "/"));

describe("takeLoginCode", () => {
  it("returns the code and takes the parameter out of the address", () => {
    window.history.replaceState(null, "", "/?login=abc123");
    expect(takeLoginCode(window)).toBe("abc123");
    expect(window.location.search).toBe("");
    expect(window.location.pathname).toBe("/");
  });

  it("keeps the path, the other parameters and the hash", () => {
    window.history.replaceState(null, "", "/board/1?a=1&login=zz&b=2#top");
    expect(takeLoginCode(window)).toBe("zz");
    expect(window.location.pathname + window.location.search + window.location.hash).toBe(
      "/board/1?a=1&b=2#top",
    );
  });

  it("does nothing without a code", () => {
    window.history.replaceState(null, "", "/inbox?x=1");
    const replace = vi.spyOn(window.history, "replaceState");
    expect(takeLoginCode(window)).toBeNull();
    expect(replace).not.toHaveBeenCalled();
    replace.mockRestore();
  });

  it("removes an empty parameter too, but signs in with nothing", () => {
    window.history.replaceState(null, "", "/?login=");
    expect(takeLoginCode(window)).toBeNull();
    expect(window.location.search).toBe("");
  });
});

describe("startLogin", () => {
  it("has taken the code out of the address before it sends it", async () => {
    window.history.replaceState(null, "", "/?login=c0de");
    let addressWhenSent = "";
    const send = vi.fn(async (code: string) => {
      addressWhenSent = window.location.search;
      expect(code).toBe("c0de");
    });
    const outcome = await startLogin(window, send);
    expect(send).toHaveBeenCalledOnce();
    expect(addressWhenSent).toBe("");
    expect(outcome).toEqual({ kind: "signed_in" });
    expect(await loginOutcome()).toEqual({ kind: "signed_in" });
  });

  it("reports a code the server does not accept", async () => {
    window.history.replaceState(null, "", "/?login=old");
    const outcome = await startLogin(window, () => Promise.reject(new ApiError(401, "no")));
    expect(outcome.kind).toBe("failed");
    expect(window.location.search).toBe("");
  });

  it("reports a server that does not answer differently", async () => {
    window.history.replaceState(null, "", "/?login=x");
    const outcome = await startLogin(window, () => Promise.reject(new ApiError(0, "down")));
    expect(outcome).toEqual({ kind: "failed", message: "The server could not be reached." });
  });

  it("sends nothing when the address has no code", async () => {
    const send = vi.fn();
    expect(await startLogin(window, send)).toEqual({ kind: "none" });
    expect(send).not.toHaveBeenCalled();
  });
});
