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
    <main className="flex min-h-full items-center justify-center bg-ground p-double">
      <form
        className="flex w-full max-w-[440px] flex-col gap-double rounded-xl border border-line bg-surface p-7"
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <div>
          <h1 className="text-[19px] font-bold text-high">Moebius</h1>
          <p className="mt-half text-sm text-normal">
            Sign in with the token of this server. It is the content of its token file, by default{" "}
            <code className="font-mono text-xs">.moebius/token</code>, or open the link the server
            printed when it started.
          </p>
        </div>
        <label className="flex flex-col gap-half text-sm text-normal" htmlFor="token">
          Token
          <Input
            id="token"
            type="password"
            autoComplete="off"
            spellCheck={false}
            autoFocus
            value={token}
            onChange={(event) => setToken(event.target.value)}
          />
        </label>
        {error && (
          <p role="alert" className="text-sm text-error">
            {error}
          </p>
        )}
        <Button type="submit" disabled={busy || token.trim() === ""}>
          Sign in
        </Button>
      </form>
    </main>
  );
}
