/**
 * The OAuth login of an upstream on the Models page (the former desktop app's `login`): the warning about the provider's terms,
 * which the person confirms before a login starts (`POST /v1/models/upstreams/{place}/login`), then the code to enter
 * at the provider or the provider's page, opened in a new tab, then whether the login holds, read again every second
 * (`GET /v1/models/upstreams/{place}/login`) while it waits. A login by the provider's page also takes the code, or
 * the address the browser ended on, pasted into a field (`POST /v1/models/upstreams/{place}/login/code`), for a
 * browser that cannot reach the server.
 *
 * A browser opens a tab only for a click, not after an answer from the server, so "I understand, log in" opens an empty
 * tab at once and sends it to the provider's page when the server has it; "Open the page" is there for a tab that was
 * blocked.
 */
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { pasteLogin, login as readLogin, startLogin } from "../../api/calls/models";
import { keys } from "../../api/keys";
import type { Login, UpstreamOAuth } from "../../api/types/models";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { Status } from "../../ui/page/Status";

/** Where the login of an upstream is on the page. */
export type LoginStep =
  | { kind: "confirming" }
  | { kind: "starting" }
  | { kind: "answered"; login: Login }
  | { kind: "failed"; message: string };

const POLL_MS = 1000;

const message = (error: unknown) =>
  error instanceof Error ? error.message : "cannot reach the server";

export function LoginRow({
  place,
  oauth,
  step,
  onStep,
}: {
  place: number;
  oauth: UpstreamOAuth;
  step: LoginStep | undefined;
  onStep: (step: LoginStep | undefined) => void;
}) {
  const queryClient = useQueryClient();
  const tab = useRef<Window | null>(null);
  const [pasted, setPasted] = useState("");
  const [pasteProblem, setPasteProblem] = useState<string | null>(null);

  // An upstream that logged in lists its models now, not when the kept list runs out.
  const ended = () => {
    void queryClient.invalidateQueries({ queryKey: keys.modelsInUse });
    void queryClient.invalidateQueries({ queryKey: keys.models });
    void queryClient.invalidateQueries({ queryKey: keys.availableDecision });
  };
  const answered = (login: Login) => {
    onStep({ kind: "answered", login });
    if (login.state !== "waiting") ended();
  };

  const waiting = step?.kind === "answered" && step.login.state === "waiting" ? step.login : null;
  const latest = useRef({ answered, onStep });
  latest.current = { answered, onStep };
  useEffect(() => {
    if (!waiting) return;
    let live = true;
    const timer = setTimeout(() => {
      readLogin(place).then(
        (login) => live && latest.current.answered(login),
        (error) => live && latest.current.onStep({ kind: "failed", message: message(error) }),
      );
    }, POLL_MS);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [waiting, place]);

  function confirm() {
    // The tab is opened by the click; the page it goes to comes with the answer.
    tab.current = oauth.flow === "code" ? window.open("", "_blank") : null;
    if (tab.current) tab.current.opener = null;
    setPasted("");
    setPasteProblem(null);
    onStep({ kind: "starting" });
    startLogin(place).then(
      (login) => {
        if (login.authorize_url && tab.current && !tab.current.closed) {
          tab.current.location.href = login.authorize_url;
        } else {
          tab.current?.close();
        }
        tab.current = null;
        answered(login);
      },
      (error) => {
        tab.current?.close();
        tab.current = null;
        onStep({ kind: "failed", message: message(error) });
      },
    );
  }

  function finish() {
    if (pasted.trim() === "") return;
    pasteLogin(place, pasted).then(
      (login) => {
        setPasted("");
        answered(login);
      },
      (error) => setPasteProblem(message(error)),
    );
  }

  const busy = step?.kind === "confirming" || step?.kind === "starting";
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <Status tone={oauth.logged_in ? "complete" : "attention"}>
            {oauth.logged_in ? "A login is kept." : "Not logged in."}
          </Status>
          <span className="min-w-0 break-words text-sm text-secondary">
            Logs in with OAuth:{" "}
            {oauth.flow === "device"
              ? "a code you enter at the provider."
              : "the provider's page in your browser."}
          </span>
        </div>
        <Button
          variant="secondary"
          size="sm"
          disabled={busy}
          title={busy ? "The login is already starting" : undefined}
          onClick={() => onStep({ kind: "confirming" })}
        >
          {oauth.logged_in ? "Log in again" : "Log in"}
        </Button>
      </div>
      <p
        data-testid="oauth-warning"
        className="m-0 max-w-prose break-words rounded-control bg-attention-fill px-3 py-2 text-sm text-attention-text"
      >
        {oauth.warning}
      </p>
      {oauth.needs && (
        <Notice tone="error">
          Requests to this provider fail: they need {oauth.needs}, which is not served yet.
        </Notice>
      )}
      {step?.kind === "confirming" && (
        <div className="flex flex-wrap gap-3">
          <Button size="sm" onClick={confirm}>
            I understand, log in
          </Button>
          <Button variant="secondary" size="sm" onClick={() => onStep(undefined)}>
            Cancel
          </Button>
        </div>
      )}
      {step?.kind === "starting" && (
        <p className="m-0 text-sm text-secondary">Starting the login</p>
      )}
      {step?.kind === "failed" && <Notice tone="error">{step.message}</Notice>}
      {step?.kind === "answered" && (
        <Answer
          login={step.login}
          pasted={pasted}
          problem={pasteProblem}
          onPasted={(text) => {
            setPasted(text);
            setPasteProblem(null);
          }}
          onFinish={finish}
          onCancel={() => {
            setPasted("");
            onStep(undefined);
          }}
        />
      )}
    </div>
  );
}

/** What the server said of a login: the code and the page while it waits, then how it ended. */
function Answer({
  login,
  pasted,
  problem,
  onPasted,
  onFinish,
  onCancel,
}: {
  login: Login;
  pasted: string;
  problem: string | null;
  onPasted: (text: string) => void;
  onFinish: () => void;
  onCancel: () => void;
}) {
  if (login.state === "logged_in") return <Notice tone="ok">{login.message}</Notice>;
  if (login.state !== "waiting") return <Notice tone="error">{login.message}</Notice>;
  const page = login.verification_uri_complete ?? login.verification_uri ?? login.authorize_url;
  return (
    <div className="flex flex-col gap-3">
      {login.user_code ? (
        <div className="flex flex-wrap items-center gap-3">
          <span
            data-testid="user-code"
            className="rounded-control bg-code px-3 py-2 font-mono text-sm font-semibold tracking-widest text-text"
          >
            {login.user_code}
          </span>
          <span className="min-w-0 flex-1 break-words text-sm text-secondary">
            Enter this code at {login.verification_uri}
          </span>
        </div>
      ) : (
        <p className="m-0 text-base text-text">
          The provider's page opened in a new tab. Log in there.
        </p>
      )}
      {page && (
        <div>
          <Button asChild variant="secondary" size="sm">
            <a href={page} target="_blank" rel="noopener noreferrer">
              {login.user_code ? "Open the page" : "Open the page again"}
            </a>
          </Button>
        </div>
      )}
      {!login.user_code && login.authorize_url && (
        <div className="flex flex-col gap-3">
          <Field
            label="Paste the code or the address you ended on"
            hint="If the browser cannot reach this machine, copy the address from its address bar after you log in."
          >
            {(id) => (
              <Input
                id={id}
                mono
                value={pasted}
                placeholder="Code, or http://localhost:1455/auth/callback?code=abc"
                onChange={(event) => onPasted(event.target.value)}
                onCommandEnter={onFinish}
              />
            )}
          </Field>
          <div className="flex flex-wrap items-center gap-3">
            <Button
              size="sm"
              disabled={pasted.trim() === ""}
              title={pasted.trim() === "" ? "Paste the code or the address first" : undefined}
              onClick={onFinish}
            >
              Finish login
            </Button>
            {problem && <Notice tone="error">{problem}</Notice>}
          </div>
        </div>
      )}
      <div className="flex flex-wrap items-center gap-3">
        <Status tone="progress">Waiting for the login</Status>
        <Button variant="secondary" size="sm" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  );
}
