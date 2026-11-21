import { expect, it } from "vitest";
import { rowAge } from "./rowAge";

it("says the age the compact way", () => {
  expect(rowAge(1000, 1030)).toBe("now");
  expect(rowAge(1000, 1000 + 125)).toBe("2m");
  expect(rowAge(1000, 1000 + 3 * 3600 + 5)).toBe("3h");
  expect(rowAge(1000, 1000 + 2 * 86_400)).toBe("2d");
  expect(rowAge(1000, 10)).toBe("now");
});
