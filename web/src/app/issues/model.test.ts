import { describe, expect, it } from "vitest";
import { card, held } from "../board/model/fixtures";
import { groupOf, iconOf, myIssueItems, shortAge } from "./model";

describe("the groups of My issues", () => {
  it("puts a card in the group of its column, a closed card in Done and an unknown column in Not started", () => {
    expect(groupOf(card(1, { column: "In progress" }))).toBe("In progress");
    expect(groupOf(card(1, { column: "Ready to merge" }))).toBe("In review");
    expect(groupOf(card(1, { column: "Backlog", closed: true }))).toBe("Done");
    expect(groupOf(card(1, { column: "My own column" }))).toBe("Not started");
  });

  it("shows a held open card as needing a person and every other card by its group", () => {
    expect(iconOf(card(1, { column: "In progress", held }))).toBe("needsYou");
    expect(iconOf(card(1, { column: "In progress" }))).toBe("inProgress");
    expect(iconOf(card(1, { column: "In review", held, closed: true }))).toBe("done");
  });

  it("lists a group row before the cards of each group that has any, in the page's order", () => {
    const items = myIssueItems(
      [
        card(1, { column: "Done" }),
        card(2, { column: "Backlog" }),
        card(3, { column: "In progress", created_at: 940 }),
        card(4, { column: "Backlog", held }),
      ],
      1000,
    );
    expect(items.map((item) => (item.type === "group" ? item.name : item.key))).toEqual([
      "In progress",
      "3",
      "Not started",
      "2",
      "4",
      "Done",
      "1",
    ]);
    const third = items.find((item) => item.key === "3");
    expect(third?.type === "row" && third.data.age).toBe("1m");
    const fourth = items.find((item) => item.key === "4");
    expect(fourth?.type === "row" && fourth.data.reason).toBe(held.reason);
  });

  it("says an age the short way", () => {
    expect([0, 59, 60, 3599, 3600, 86_399, 86_400, 200_000].map((s) => shortAge(s, 0))).toEqual([
      "now",
      "now",
      "1m",
      "59m",
      "1h",
      "23h",
      "1d",
      "2d",
    ]);
    expect(shortAge(0, 10)).toBe("now");
  });
});
