/** The browser's sign-in: `POST`, `GET` and `DELETE /v1/session`. */
import { del, get, post } from "../http";
import type { Session, SignIn } from "../types/server";

/** Signs in with the server's token or a one-time login code; the server answers with the session cookie. */
export function signIn(credentials: SignIn): Promise<void> {
  return post<void>("/v1/session", credentials);
}

/** Whether the browser is signed in; a 401 `ApiError` when it is not. */
export function getSession(): Promise<Session> {
  return get<Session>("/v1/session");
}

export function signOut(): Promise<void> {
  return del<void>("/v1/session");
}
