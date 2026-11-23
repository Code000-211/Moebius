/** The calls about a person's logins on the forges: `/v1/logins`. */
import { get, put } from "../http";
import type { Logins } from "../types/logins";

/** The logins of the person asking; the empty string for a forge they have said nothing about. */
export function logins(): Promise<Logins> {
  return get<Logins>("/v1/logins");
}

/** Replaces the logins and answers with them as the server kept them. */
export function saveLogins(next: Logins): Promise<Logins> {
  return put<Logins>("/v1/logins", next);
}
