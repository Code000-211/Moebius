/** The end-to-end tests open the built app: stop with a clear message when `pnpm build` has not run. */
import { existsSync } from "node:fs";
import { resolve } from "node:path";

export default function setup() {
  if (!existsSync(resolve(import.meta.dirname, "../dist/index.html"))) {
    throw new Error("web/dist is missing: run `pnpm build` before `pnpm e2e`");
  }
}
