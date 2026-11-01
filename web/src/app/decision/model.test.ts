import { describe, expect, it } from "vitest";
import type { Export } from "../../api/types/improve";
import {
  agreementLine,
  exportRoute,
  judgements,
  percent,
  recallLine,
  shadowsLine,
  when,
} from "./model";

const counts = (kind: string, count = 1) => ({
  kind,
  count,
  with_outcome: 0,
  by_person: 0,
  held_out: 0,
});

describe("a share", () => {
  it("is a percentage, with a decimal only when it has one", () => {
    expect(percent(0.97)).toBe("97%");
    expect(percent(0.965)).toBe("96.5%");
    expect(percent(0.975)).toBe("97.5%");
    expect(percent(1)).toBe("100%");
    expect(percent(0.9)).toBe("90%");
  });

  it("is told against the share a takeover needs", () => {
    expect(agreementLine(0.85, 0.95)).toBe("85% (needs 95%)");
  });
});

describe("a time", () => {
  it("names the day and keeps the UTC clock", () => {
    expect(when(1_793_182_500, "2026-10-28")).toBe("Today 10:15 UTC");
    expect(when(1_793_182_500, "2026-10-29")).toBe("Yesterday 10:15 UTC");
    expect(when(1_793_182_500, "2026-10-31")).toBe("28 Oct 10:15 UTC");
    expect(when(1_793_182_500, "2027-01-01")).toBe("28 Oct 2026 10:15 UTC");
    expect(when(951_782_400 + 86_399, "2001-05-01")).toBe("29 Feb 2000 23:59 UTC");
  });
});

describe("the exports", () => {
  const exports: Export[] = [
    { id: 2, at: 2, kinds: [counts("merge"), counts("tool_call")] },
    { id: 1, at: 1, kinds: [counts("merge"), counts("approve_issue")] },
  ];

  it("name the route of the newest file of a kind", () => {
    expect(exportRoute(exports, "tool_call")).toBe("GET /v1/exports/2/tool_call.jsonl");
    expect(exportRoute(exports, "approve_issue")).toBe("GET /v1/exports/1/approve_issue.jsonl");
    expect(exportRoute(exports, "retry")).toBeNull();
  });

  it("count the judgements of all kinds", () => {
    expect(judgements([counts("a", 14), counts("b", 9)])).toBe(23);
  });
});

describe("a shadowed kind", () => {
  const kind = {
    kind: "merge",
    shadows: 40,
    agreement: 0.85,
    failed: 3,
    dropped: 2,
    routed: false,
  };

  it("tells its shadows, the failed and the dropped", () => {
    expect(shadowsLine(kind)).toBe("40, 3 failed, 2 dropped");
    expect(shadowsLine({ ...kind, failed: 0, dropped: 0, shadows: 0 })).toBe("0");
  });

  it("is measured by recall for localize", () => {
    expect(recallLine(kind)).toBeNull();
    expect(recallLine({ ...kind, kind: "localize", recall: 0.8, baseline_recall: 0.5 })).toBe(
      "Recall 80% against 50% of the hints",
    );
    expect(recallLine({ ...kind, recall: 0.5 })).toBe("Recall 50%");
  });
});
