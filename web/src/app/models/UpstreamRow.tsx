/**
 * One upstream on the Models page, the heart of it: a word for whether it answers and a word for where its key comes
 * from, then the address, the protocol and the key, with the login of one that uses OAuth under the fields. Also the
 * line that says an upstream did not answer and what to do about it.
 */

import type { AvailableModels } from "../../api/types/chat";
import type { UpstreamOAuth } from "../../api/types/models";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { plural } from "../../ui/lib/format";
import { Field, Select } from "../../ui/page/Field";
import { StateLabel } from "../../ui/page/StateLabel";
import { couldNotAsk } from "../harness/ModelField";
import { KeyField } from "./KeyField";
import { LoginRow, type LoginStep } from "./LoginRow";
import { type KeyChoice, PROTOCOLS, type UpstreamForm } from "./model";

/** An upstream that did not answer, as `GET /v1/models/available` reported it. */
export type AvailableError = AvailableModels["errors"][number];

/** An upstream that did not answer: what the server said, and what to do about it. */
export function Failure({ upstream, message }: AvailableError) {
  return (
    <div className="flex flex-col gap-1">
      <span className="break-words text-sm text-error-text">{couldNotAsk(upstream, message)}</span>
      <span className="text-sm text-secondary">Check the address and the key, then refresh.</span>
    </div>
  );
}

/**
 * One upstream, and the heart of the page: a word for whether it answers and a word for where its key comes from,
 * then the address, the protocol and the key. One that did not answer says what the server said and what to do, and
 * one that logs in carries its login under the fields.
 */
export function UpstreamRow({
  upstream,
  place,
  oauth,
  step,
  failure,
  served,
  asked,
  secrets,
  onPatch,
  onKey,
  onStep,
  onRemove,
}: {
  upstream: UpstreamForm;
  place: number;
  oauth: UpstreamOAuth | null | undefined;
  step: LoginStep | undefined;
  failure: AvailableError | undefined;
  /** How many models this upstream served the last time the app asked. */
  served: number;
  /** Whether the app has asked the upstreams at all. */
  asked: boolean;
  secrets: string[];
  onPatch: (patch: Partial<UpstreamForm>) => void;
  onKey: (change: Partial<KeyChoice>) => void;
  onStep: (step: LoginStep | undefined) => void;
  onRemove: () => void;
}) {
  const needsAddress = upstream.baseUrl.trim() === "";
  const answer = failure
    ? { tone: "error" as const, text: "Did not answer" }
    : served > 0
      ? { tone: "complete" as const, text: plural(served, "model", "models") }
      : needsAddress
        ? { tone: "attention" as const, text: "Needs an address" }
        : asked
          ? { tone: "neutral" as const, text: "No models yet" }
          : { tone: "neutral" as const, text: "Not asked yet" };
  return (
    <section data-testid="upstream" className="flex flex-col gap-4 rounded-control bg-sunken p-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <StateLabel tone={answer.tone}>{answer.text}</StateLabel>
          {oauth?.needs && <StateLabel tone="attention">Needs {oauth.needs}</StateLabel>}
        </div>
        <Button
          variant="danger"
          size="sm"
          aria-label={`Remove provider ${place + 1}`}
          onClick={onRemove}
        >
          Remove
        </Button>
      </div>
      <div className="grid items-start gap-4 sm:grid-cols-2">
        <Field label="Protocol">
          {(id) => (
            <Select
              id={id}
              aria-label="Protocol"
              options={[...PROTOCOLS]}
              value={upstream.protocol}
              onChange={(protocol) => onPatch({ protocol })}
            />
          )}
        </Field>
        <Field label="Base URL" hint="The address Moebius sends the requests to.">
          {(id) => (
            <Input
              id={id}
              mono
              aria-label="Base URL"
              value={upstream.baseUrl}
              placeholder="http://127.0.0.1:8080"
              onChange={(event) => onPatch({ baseUrl: event.target.value })}
            />
          )}
        </Field>
      </div>
      {oauth ? (
        // Not a `Field` with an id: there is no control here, so the label would point at nothing.
        <Field label="Key">
          <span className="py-2 text-sm text-secondary">None: it logs in with OAuth</span>
        </Field>
      ) : (
        <KeyField
          label={`Key of provider ${place + 1}`}
          choice={upstream.key}
          secrets={secrets}
          onChange={onKey}
        />
      )}
      {failure && <Failure upstream={failure.upstream} message={failure.message} />}
      {oauth && upstream.origin !== null && (
        <LoginRow place={upstream.origin} oauth={oauth} step={step} onStep={onStep} />
      )}
    </section>
  );
}
