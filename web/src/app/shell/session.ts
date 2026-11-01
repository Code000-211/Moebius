/**
 * How a session ends, from wherever it is asked for. Ending one is not only calling the API: every answer cached about
 * the old session goes with it, and the session is read again, which is what brings the sign-in page back. The Shell
 * holds that page, so the rule lives here rather than in the screen that happens to be asked.
 */
import type { QueryClient } from "@tanstack/react-query";
import { signOut } from "../../api/calls/session";
import { keys } from "../../api/keys";

export async function leaveSession(queryClient: QueryClient): Promise<void> {
  try {
    await signOut();
  } finally {
    queryClient.removeQueries({ predicate: (query) => query.queryKey[0] !== keys.session[0] });
    await queryClient.invalidateQueries({ queryKey: keys.session });
  }
}
