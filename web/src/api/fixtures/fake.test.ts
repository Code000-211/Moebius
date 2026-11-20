/**
 * The fake server of the end-to-end tests answers what the real server answers: the shapes of its workflow routes
 * are held to `workflows.json`, which a Rust test of the real routes (`server/api/workflows.rs`) holds to the real
 * server in turn. If the server's answers change, that test fails until the fixtures say the new keys, and this one
 * fails until the fake does.
 */
import type { IncomingMessage, ServerResponse } from "node:http";
import { describe, expect, it } from "vitest";
import { createWorkflowsFake } from "../../../e2e/fake/workflows";
import fixtures from "./workflows.json";

/** The keys of `real` are those of `fixture` all the way down; null stands for anything on either side, and the
 * keys that hold a value of the workflow's own making are not looked into. */
function sameKeys(real: unknown, fixture: unknown, at: string): void {
  const free = new Set(["value", "input", "output", "asks", "params"]);
  if (real === null || fixture === null) return;
  if (Array.isArray(real) && Array.isArray(fixture)) {
    if (real[0] !== undefined && fixture[0] !== undefined) sameKeys(real[0], fixture[0], `${at}[]`);
    return;
  }
  if (typeof real === "object" && typeof fixture === "object") {
    const a = real as Record<string, unknown>;
    const b = fixture as Record<string, unknown>;
    expect(Object.keys(a).sort(), `the keys of ${at}`).toEqual(Object.keys(b).sort());
    for (const key of Object.keys(a)) {
      if (!free.has(key)) sameKeys(a[key], b[key], `${at}.${key}`);
    }
    return;
  }
  expect(typeof real, `the type of ${at}`).toBe(typeof fixture);
}

async function ask(path: string): Promise<{ status: number; body: unknown }> {
  const fake = createWorkflowsFake();
  let status = 0;
  let text = "";
  const response = {
    writeHead(code: number) {
      status = code;
    },
    end(body: string) {
      text = body;
    },
  } as unknown as ServerResponse;
  const request = { method: "GET", url: path } as IncomingMessage;
  await fake.handle(request, response, path.split("?")[0] ?? path);
  return { status, body: text.startsWith("{") || text.startsWith("[") ? JSON.parse(text) : text };
}

describe("the fake server's workflow answers", () => {
  it("have the keys of the real server's", async () => {
    sameKeys((await ask("/v1/workflows")).body, fixtures.list, "list");
    sameKeys((await ask("/v1/workflows/release")).body, fixtures.show, "show");
    sameKeys((await ask("/v1/workflow-runs?workflow=release")).body, fixtures.runs, "runs");
    sameKeys((await ask("/v1/workflow-runs/41")).body, fixtures.run, "run");
  });

  it("answers a workflow that does not exist as not found", async () => {
    expect((await ask("/v1/workflows/nothing")).status).toBe(404);
  });
});
