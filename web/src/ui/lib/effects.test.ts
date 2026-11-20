import { describe, expect, it } from "vitest";
import { blurAllowed } from "./effects";

describe("blurAllowed", () => {
  it.each([
    "ANGLE (Google, Vulkan 1.3.0 (SwiftShader Device (Subzero)), SwiftShader driver)",
    "llvmpipe (LLVM 15.0.7, 256 bits)",
    "Gallium 0.4 on softpipe",
    "Microsoft Basic Render Driver",
    "",
    null,
    undefined,
  ])("says no for software or unknown: %s", (name) => {
    expect(blurAllowed(name)).toBe(false);
  });

  it.each([
    "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 Direct3D11 vs_5_0 ps_5_0, D3D11)",
    "Apple M2",
    "Mesa Intel(R) UHD Graphics 620 (KBL GT2)",
    "AMD Radeon Pro 5500M OpenGL Engine",
  ])("says yes for a GPU: %s", (name) => {
    expect(blurAllowed(name)).toBe(true);
  });
});
