// @vitest-environment node
/**
 * xterm is the heaviest thing the app holds, so it is fetched only when a terminal is opened. These tests hold that
 * in place: the two modules that speak xterm are `Terminal.tsx` and nothing else, and every screen that shows a shell
 * goes through `LazyTerminal`.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const sources: string[] = [];
function walk(dir: string): void {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) walk(path);
    else if (/\.tsx?$/.test(entry) && !entry.includes(".test.")) sources.push(path);
  }
}
walk("src");

const mentions = (text: string, file: string) =>
  sources
    .filter((each) => readFileSync(each, "utf8").includes(text))
    .map((each) => each.replace(/\\/g, "/").replace(`src/terminal/`, ""))
    .filter((each) => each !== file);

describe("the terminal is loaded when it opens", () => {
  it("leaves xterm to the one module that holds it", () => {
    expect(mentions('"@xterm/', "Terminal.tsx")).toEqual([]);
  });

  it("keeps every screen off the xterm module itself", () => {
    expect(mentions('from "./Terminal"', "LazyTerminal.tsx")).toEqual([]);
    expect(mentions('from "../terminal/Terminal"', "LazyTerminal.tsx")).toEqual([]);
    expect(mentions('from "../../terminal/Terminal"', "LazyTerminal.tsx")).toEqual([]);
  });
});
