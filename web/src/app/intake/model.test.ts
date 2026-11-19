import { describe, expect, it } from "vitest";
import type { IntakeRequest } from "../../api/types/intake";
import { decides, open, ordered, statusWords } from "./model";

const request = (id: number, status: IntakeRequest["status"], reason = ""): IntakeRequest => ({
  id,
  project_id: 1,
  requester: "kim",
  text: "Dark mode",
  title: "",
  body: "",
  status,
  run_id: null,
  card_id: null,
  reason,
  created_at: 0,
  decided_at: null,
});

describe("intake requests", () => {
  it("says what a declined request was declined for", () => {
    expect(statusWords(request(1, "declined", "Out of scope"))).toBe("Declined: Out of scope");
    expect(statusWords(request(1, "drafted"))).toBe("Waiting for a member");
  });

  it("is open until it is accepted or declined", () => {
    expect(
      ["grilling", "drafted", "accepted", "declined"].map((s) => open(request(1, s as never))),
    ).toEqual([true, true, false, false]);
  });

  it("lets a member and an owner decide, never a viewer", () => {
    expect([decides("owner"), decides("member"), decides("viewer"), decides(undefined)]).toEqual([
      true,
      true,
      false,
      false,
    ]);
  });

  it("puts what waits for a decision first", () => {
    const all = [
      request(4, "declined"),
      request(3, "drafted"),
      request(2, "accepted"),
      request(1, "grilling"),
    ];
    expect(ordered(all).map((r) => r.id)).toEqual([3, 1, 4, 2]);
  });
});
