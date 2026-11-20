/** The logic of the Get started page: the page that fixes each step, (the former desktop app's `screens/start`). */
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
