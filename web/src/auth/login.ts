/**
 * The `?login=<code>` link the server prints: on load the code is taken out of the address (before anything else
 * runs, so it is not kept in the history or shown) and sent to `POST /v1/session`. The result waits here until the
 * session query asks for it.
 */
import { signIn } from "../api/calls/session";
import { isApiError } from "../api/http";

/** Removes `login` from the address with `history.replaceState` and returns its value, or null when there is none. */
export function takeLoginCode(win: Pick<Window, "location" | "history">): string | null {
  const url = new URL(win.location.href);
  const code = url.searchParams.get("login");
  if (code === null) return null;
  url.searchParams.delete("login");
  win.history.replaceState(win.history.state, "", url.pathname + url.search + url.hash);
  return code === "" ? null : code;
}

export type LoginOutcome =
  | { kind: "none" }
  | { kind: "signed_in" }
  /** The link was not accepted (used, expired or wrong) or the server did not answer. */
  | { kind: "failed"; message: string };

let pending: Promise<LoginOutcome> | null = null;
let last: LoginOutcome = { kind: "none" };

/** Takes the code from the address and signs in with it. Called once, before the app renders. */
export function startLogin(
  win: Pick<Window, "location" | "history">,
  send: (code: string) => Promise<void> = (code) => signIn({ code }),
): Promise<LoginOutcome> {
  const code = takeLoginCode(win);
  if (code === null) {
    pending = null;
    return Promise.resolve({ kind: "none" });
  }
  pending = send(code).then(
    (): LoginOutcome => ({ kind: "signed_in" }),
    (error: unknown): LoginOutcome => ({
      kind: "failed",
      message:
        isApiError(error) && !error.unreachable
          ? "This sign-in link was not accepted. Paste the token instead."
          : "The server could not be reached.",
    }),
  );
  return pending;
}

/** Waits for the sign-in that `startLogin` began, if any. */
export async function loginOutcome(): Promise<LoginOutcome> {
  if (pending) {
    last = await pending;
    pending = null;
  }
  return last;
}

/** What the last `?login=` sign-in came to, for the sign-in page to explain. */
export function lastLoginOutcome(): LoginOutcome {
  return last;
}
