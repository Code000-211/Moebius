/**
 * The sign-in page: the person pastes the server's token (the content of its token file), or arrives through the
 * `?login=` link the server prints, which `auth/login.ts` has already used by the time this page could show.
 */
import { useState } from "react";
import { signIn } from "../api/calls/session";
import { isApiError } from "../api/http";
import { lastLoginOutcome } from "../auth/login";
import { Button } from "../ui/components/Button";
import { Input } from "../ui/components/Input";
import { Field } from "../ui/page/Field";
import { Notice } from "../ui/page/Notice";

/** The sentence for a refused or failed sign-in. */
export function signInMessage(error: unknown): string {
  if (isApiError(error)) {
    if (error.unreachable) return "The server could not be reached.";
    if (error.status === 401) return "That token is not accepted.";
    if (error.status === 403) {
      return "That is a run token of an agent. Use the token of the server.";
    }
    return error.message;
  }
  return "Signing in failed.";
}

export function SignIn({ onSignedIn }: { onSignedIn: () => void }) {
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(() => {
    const outcome = lastLoginOutcome();
    return outcome.kind === "failed" ? outcome.message : null;
  });

  async function submit() {
    if (token.trim() === "" || busy) return;
    setBusy(true);
    setError(null);
    try {
      await signIn({ token: token.trim() });
      onSignedIn();
    } catch (failure) {
      setError(signInMessage(failure));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="flex min-h-full items-center justify-center bg-background p-6">
      <form
        className="flex w-full max-w-(--connect-width) flex-col gap-4 rounded-card bg-surface p-8 shadow-card"
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <div className="flex flex-col gap-2">
          <h1 className="m-0 text-xl font-semibold tracking-tight text-text">Moebius</h1>
          <p className="m-0 text-base leading-relaxed text-secondary">
            Sign in with the token of this server, or open the link the server printed when it
            started. The token is the content of{" "}
            <code className="rounded-small bg-sunken px-2 py-0.5 font-mono text-sm text-text">
              .moebius/token
            </code>
            .
          </p>
        </div>
        <Field label="Token">
          {(id) => (
            <Input
              id={id}
              type="password"
              autoComplete="off"
              spellCheck={false}
              autoFocus
              aria-invalid={error !== null}
              value={token}
              onChange={(event) => setToken(event.target.value)}
            />
          )}
        </Field>
        {error && <Notice tone="error">{error}</Notice>}
        <Button type="submit" size="md" disabled={busy || token.trim() === ""}>
          {busy ? "Signing in" : "Sign in"}
        </Button>
      </form>
    </main>
  );
}
