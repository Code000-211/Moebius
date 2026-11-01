/** The logic of the Get started page: the page that fixes each step, and what the page says under its title (the desktop app's `screens/start`). */
import type { Start } from "../../api/types/server";

export interface Fix {
  /** The path of the page that fixes the step. */
  to: string;
  label: string;
}

/** The page that fixes a step, and what its button says; the harness step is fixed on the page itself. */
export function fix(key: string): Fix | null {
  switch (key) {
    case "upstream":
      return { to: "/models", label: "Open Models" };
    case "pi_bolt":
      return { to: "/settings", label: "Open Settings" };
    case "coder":
      return { to: "/agents", label: "Open Agents" };
    case "repository":
      return { to: "/projects/new", label: "Add repository" };
    default:
      return null;
  }
}

/** What the page says under its title. */
export function context(answer: Start | undefined): string {
  if (!answer) return "Reading what the server still needs.";
  return answer.done
    ? "Everything is set up. Cards can run."
    : "A few things have to be set up before the first card can run.";
}
