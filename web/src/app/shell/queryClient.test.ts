import { describe, expect, it, vi } from "vitest";
import { ApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { makeQueryClient } from "./queryClient";

describe("the app's query client", () => {
  it("reads the session again when a change or a read answers 401, and not for any other error", async () => {
    const client = makeQueryClient();
    const spy = vi.spyOn(client, "invalidateQueries");
    const unauthorized = new ApiError(401, "a valid token is needed");

    await client
      .fetchQuery({
        queryKey: ["people"],
        queryFn: () => Promise.reject(unauthorized),
      })
      .catch(() => {});
    expect(spy).toHaveBeenCalledWith({ queryKey: keys.session });

    spy.mockClear();
    await client
      .fetchQuery({ queryKey: ["x"], queryFn: () => Promise.reject(new ApiError(404, "no")) })
      .catch(() => {});
    await client
      .fetchQuery({ queryKey: keys.session, queryFn: () => Promise.reject(unauthorized) })
      .catch(() => {});
    expect(spy).not.toHaveBeenCalled();

    await client
      .getMutationCache()
      .build(client, { mutationFn: () => Promise.reject(unauthorized) })
      .execute(undefined)
      .catch(() => {});
    expect(spy).toHaveBeenCalledWith({ queryKey: keys.session });
  });
});
