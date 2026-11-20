/**
 * The app's query cache and what it does with a `401`. A signed-in app whose token stops working (the person was
 * disabled, the token revoked) must not look usable until a reload: any request that answers 401, a read or a change,
 * reads the session again, which answers 401 too and brings the sign-in page (`app/App.tsx`).
 */
import { MutationCache, QueryCache, QueryClient } from "@tanstack/react-query";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { isStreamUp } from "../../live/invalidation";

export function makeQueryClient(): QueryClient {
  const client: QueryClient = new QueryClient({
    queryCache: new QueryCache({ onError: (error, query) => leave(error, query.queryKey) }),
    mutationCache: new MutationCache({ onError: (error) => leave(error, []) }),
    defaultOptions: {
      queries: {
        staleTime: 30_000,
        retry: false,
        // The event stream keeps the snapshot current, so coming back to the tab does not read it again while it is up.
        refetchOnWindowFocus: (query) =>
          !(query.queryKey[0] === keys.snapshot[0] && isStreamUp(client)),
      },
    },
  });
  function leave(error: unknown, key: readonly unknown[]) {
    if (!isApiError(error) || !error.unauthorized || key[0] === keys.session[0]) return;
    void client.invalidateQueries({ queryKey: keys.session });
  }
  return client;
}
