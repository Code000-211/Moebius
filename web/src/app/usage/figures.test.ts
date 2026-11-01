import { describe, expect, it } from "vitest";
import {
  bytes,
  dayNumber,
  duration,
  grouped,
  milliseconds,
  monthAndDay,
  share,
  short,
  WEEKDAYS,
  weekday,
} from "./figures";

describe("figures", () => {
  it("tells bytes in three figures", () => {
    expect(bytes(640)).toBe("640 B");
    expect(bytes(12_400)).toBe("12.4 KB");
    expect(bytes(920_000)).toBe("920 KB");
    expect(bytes(3_400_000)).toBe("3.4 MB");
  });

  it("tells numbers in three figures, with separators, and times as hours, minutes or seconds", () => {
    expect(short(950)).toBe("950");
    expect(short(960_000)).toBe("960k");
    expect(short(1_840_000)).toBe("1.84M");
    expect(short(8_000_000)).toBe("8M");
    expect(short(148_000_000)).toBe("148M");
    expect(grouped(1284)).toBe("1,284");
    expect(grouped(12)).toBe("12");
    expect(grouped(-1234567)).toBe("-1,234,567");
    expect(duration(45)).toBe("45 s");
    expect(duration(38 * 60)).toBe("38 m");
    expect(duration(3 * 3600 + 12 * 60)).toBe("3 h 12 m");
    expect(milliseconds(412)).toBe("412 ms");
    expect(milliseconds(5200)).toBe("5 s");
  });

  it("tells a share, and 0 of nothing", () => {
    expect(share(61, 100)).toBe("61%");
    expect(share(5, 0)).toBe("0%");
  });

  it("numbers the days with their weekday and month", () => {
    const day = dayNumber("2026-10-28");
    expect(day).toBe(20_754);
    expect(WEEKDAYS[weekday(day as number)]).toBe("Wed");
    expect(monthAndDay(day as number)).toEqual([9, 28]);
    expect(monthAndDay(dayNumber("2024-02-29") as number)).toEqual([1, 29]);
    expect(dayNumber("nonsense")).toBeNull();
  });
});
