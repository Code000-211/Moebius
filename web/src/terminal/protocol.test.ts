import { describe, expect, it } from "vitest";
import {
  binaryBytes,
  endMessage,
  inputBytes,
  MAX_CELLS,
  outputBytes,
  resizeMessage,
  terminalUrl,
} from "./protocol";

describe("terminalUrl", () => {
  it("uses the scheme of the page and its host", () => {
    expect(terminalUrl({ protocol: "http:", host: "127.0.0.1:7171" }, "/v1/terminal")).toBe(
      "ws://127.0.0.1:7171/v1/terminal",
    );
    expect(
      terminalUrl({ protocol: "https:", host: "moebius.example" }, "/v1/runs/3/terminal"),
    ).toBe("wss://moebius.example/v1/runs/3/terminal");
  });
});

describe("inputBytes and binaryBytes", () => {
  it("sends text as UTF-8", () => {
    expect([...inputBytes("a\r")]).toEqual([97, 13]);
    expect([...inputBytes("é")]).toEqual([0xc3, 0xa9]);
    expect([...inputBytes("\x1b[200~x\x1b[201~")].slice(0, 6)).toEqual([27, 91, 50, 48, 48, 126]);
  });

  it("sends binary data as the bytes of its characters", () => {
    expect([...binaryBytes("\x1b[M\xff\x80")]).toEqual([27, 91, 77, 255, 128]);
  });
});

describe("resizeMessage", () => {
  it("is the JSON the server reads", () => {
    expect(JSON.parse(resizeMessage(100, 30) as string)).toEqual({
      type: "resize",
      cols: 100,
      rows: 30,
    });
  });

  it("refuses sizes the server cannot take", () => {
    for (const [cols, rows] of [
      [0, 24],
      [80, 0],
      [-1, 5],
      [80.5, 24],
      [Number.NaN, 24],
      [MAX_CELLS + 1, 24],
    ] as const) {
      expect(resizeMessage(cols, rows)).toBeNull();
    }
    expect(resizeMessage(MAX_CELLS, 1)).not.toBeNull();
  });
});

describe("outputBytes", () => {
  it("reads binary messages and ignores text", () => {
    const buffer = new Uint8Array([104, 105]).buffer;
    expect([...(outputBytes(buffer) as Uint8Array)]).toEqual([104, 105]);
    expect(outputBytes("text")).toBeNull();
    expect(outputBytes(new Blob(["x"]))).toBeNull();
  });
});

describe("endMessage", () => {
  it("says that only a person can do it when the terminal never opened", () => {
    expect(endMessage(false)).toContain("Only a person can do this");
    expect(endMessage(true)).toBe("The shell ended.");
  });
});
