/**
 * The root of the app, a route that wraps every page: it asks the server whether the browser is signed in and shows
 * the sign-in page (401), a screen for a server that does not answer, or the shell with the page inside it. Once in the
 * shell, a server that stops answering shows a banner and keeps what was on screen.
 */
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { getSession } from "../api/calls/session";
import { isApiError } from "../api/http";
import { keys } from "../api/keys";
import { loginOutcome } from "../auth/login";
import { LiveProvider } from "../live/LiveProvider";
import { Loader } from "../ui/components/Loader";
import { SignIn } from "./SignIn";
import { Shell } from "./shell/Shell";
import { Unreachable } from "./shell/Unreachable";

/** How long to wait before asking again while the server does not answer. */
const RETRY_MS = 3000;

export function App() {
  const queryClient = useQueryClient();
  const session = useQuery({
    queryKey: keys.session,
    queryFn: async () => {
      await loginOutcome();
      return getSession();
    },
    retry: false,
    refetchInterval: (query) =>
      isApiError(query.state.error) && query.state.error.unreachable ? RETRY_MS : false,
  });

  const error = session.error;
  if (isApiError(error) && error.unauthorized) {
    return (
      <SignIn
        onSignedIn={() => queryClient.setQueryData(keys.session, { signed_in: true as const })}
      />
    );
  }
  if (session.data) {
    return (
      <LiveProvider>
        <Shell
          unreachable={isApiError(error) && error.unreachable}
          onRetry={() => void session.refetch()}
        />
      </LiveProvider>
    );
  }
  if (isApiError(error)) {
    return <Unreachable retrying={session.isFetching} onRetry={() => void session.refetch()} />;
  }
  return (
    <main className="flex min-h-full items-center justify-center bg-background p-6">
      <Loader message="Connecting to the server" />
    </main>
  );
}
