import { describe, expect, it, vi } from "vitest";
import { ApiError, get, post, request, UNREACHABLE } from "./http";

const answer = (status: number, body = "") =>
  new Response(status === 204 ? null : body, { status });

describe("request", () => {
  it("sends the cookie and X-Moebius on every request", async () => {
    const fake = vi.fn().mockResolvedValue(answer(200, "{}"));
    await get("/v1/health", { fetch: fake });
    const [path, init] = fake.mock.calls[0] as [string, RequestInit];
    expect(path).toBe("/v1/health");
    expect(init.credentials).toBe("same-origin");
    expect((init.headers as Record<string, string>)["X-Moebius"]).toBe("1");
    expect(init.method).toBe("GET");
  });

  it("sends a body as JSON", async () => {
    const fake = vi.fn().mockResolvedValue(answer(204));
    await post("/v1/session", { token: "t" }, { fetch: fake });
    const [, init] = fake.mock.calls[0] as [string, RequestInit];
    expect((init.headers as Record<string, string>)["Content-Type"]).toBe("application/json");
    expect(init.body).toBe('{"token":"t"}');
  });

  it("reads a JSON answer and treats 204 as no value", async () => {
    const version = await get<{ version: string }>("/v1/health", {
      fetch: vi.fn().mockResolvedValue(answer(200, '{"version":"1"}')),
    });
    expect(version.version).toBe("1");
    const nothing = await request("DELETE", "/v1/session", {
      fetch: vi.fn().mockResolvedValue(answer(204)),
    });
    expect(nothing).toBeUndefined();
  });

  it("turns a failed answer into an ApiError with the status and the server's message", async () => {
    const fake = vi.fn().mockResolvedValue(answer(401, "a valid token is needed\n"));
    const error = await get("/v1/snapshot", { fetch: fake }).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiError);
    const api = error as ApiError;
    expect(api.status).toBe(401);
    expect(api.message).toBe("a valid token is needed");
    expect(api.unauthorized).toBe(true);
    expect(api.unreachable).toBe(false);
  });

  it("says what the status was when the server sent no message", async () => {
    const error = (await get("/v1/x", {
      fetch: vi.fn().mockResolvedValue(answer(500)),
    }).catch((e: unknown) => e)) as ApiError;
    expect(error.message).toBe("the server answered 500");
  });

  it("turns a network failure into an unreachable ApiError", async () => {
    const error = (await get("/v1/x", {
      fetch: vi.fn().mockRejectedValue(new TypeError("Failed to fetch")),
    }).catch((e: unknown) => e)) as ApiError;
    expect(error.status).toBe(UNREACHABLE);
    expect(error.unreachable).toBe(true);
    expect(error.unauthorized).toBe(false);
  });

  it("lets an abort through as it is", async () => {
    const abort = new DOMException("aborted", "AbortError");
    const error = await get("/v1/x", { fetch: vi.fn().mockRejectedValue(abort) }).catch(
      (e: unknown) => e,
    );
    expect(error).toBe(abort);
  });

  it("rejects an answer that is not JSON", async () => {
    const error = (await get("/v1/x", {
      fetch: vi.fn().mockResolvedValue(answer(200, "<html>")),
    }).catch((e: unknown) => e)) as ApiError;
    expect(error).toBeInstanceOf(ApiError);
    expect(error.message).toBe("the server's answer is not JSON");
  });
});
