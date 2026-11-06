/**
 * "Log in to a provider" on the Models page (the former desktop app's `provider`): the providers the server has built in
 * (`GET /v1/models/providers`) and "Other" with the fields of an `oauth` table. Choosing one adds the upstream through
 * `PUT /v1/settings` (live, no restart); the row of the new upstream then asks for the confirmation of the warning and
 * logs in.
 */
import { useState } from "react";
import type { Provider } from "../../api/types/models";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Field, Select } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import {
  blankOther,
  FLOWS,
  type OtherForm,
  otherUpstream,
  PROTOCOLS,
  presetUpstream,
} from "./model";

type Upstream = Record<string, unknown>;

export function ProviderPanel({
  providers,
  adding,
  problem,
  onAdd,
  onCancel,
}: {
  providers: Provider[];
  /** The upstream is being added. */
  adding: boolean;
  problem: string | null;
  onAdd: (upstream: Upstream) => void;
  onCancel: () => void;
}) {
  const [other, setOther] = useState<OtherForm | null>(null);
  const [missing, setMissing] = useState<string | null>(null);
  const edit = (patch: Partial<OtherForm>) => {
    setOther((held) => held && { ...held, ...patch });
    setMissing(null);
  };

  if (other) {
    const urlLabel = other.flow === "device" ? "Device URL" : "Authorize URL";
    return (
      <div className="flex flex-col gap-4" data-testid="provider-panel">
        <h3 className="m-0 text-lg font-semibold tracking-tight text-text">
          A provider with an OAuth login
        </h3>
        <div className="grid items-start gap-4 sm:grid-cols-2">
          <Field label="Protocol">
            {(id) => (
              <Select
                id={id}
                options={[...PROTOCOLS]}
                value={other.protocol}
                onChange={(protocol) => edit({ protocol })}
              />
            )}
          </Field>
          <Field label="Base URL">
            {(id) => (
              <Input
                id={id}
                mono
                value={other.baseUrl}
                placeholder="https://api.example.com"
                onChange={(event) => edit({ baseUrl: event.target.value })}
              />
            )}
          </Field>
        </div>
        <div className="grid items-start gap-4 sm:grid-cols-2">
          <Field label="Flow">
            {(id) => (
              <Select
                id={id}
                options={[...FLOWS]}
                value={other.flow}
                onChange={(flow) => edit({ flow })}
              />
            )}
          </Field>
          <Field label="Client ID">
            {(id) => (
              <Input
                id={id}
                mono
                value={other.clientId}
                onChange={(event) => edit({ clientId: event.target.value })}
              />
            )}
          </Field>
        </div>
        <Field label={urlLabel}>
          {(id) => (
            <Input
              id={id}
              mono
              value={other.url}
              placeholder="https://login.example.com/authorize"
              onChange={(event) => edit({ url: event.target.value })}
            />
          )}
        </Field>
        <Field label="Token URL">
          {(id) => (
            <Input
              id={id}
              mono
              value={other.tokenUrl}
              placeholder="https://login.example.com/token"
              onChange={(event) => edit({ tokenUrl: event.target.value })}
            />
          )}
        </Field>
        <Field label="Scopes, separated by spaces">
          {(id) => (
            <Input
              id={id}
              mono
              value={other.scopes}
              onChange={(event) => edit({ scopes: event.target.value })}
            />
          )}
        </Field>
        {(missing ?? problem) && <Notice tone="error">{missing ?? problem}</Notice>}
        <div className="flex flex-wrap items-center gap-3">
          <Button
            size="sm"
            disabled={adding}
            title={adding ? "The provider is being added" : undefined}
            onClick={() => {
              const upstream = otherUpstream(other, Math.floor(Date.now() / 1000));
              if (typeof upstream === "string") setMissing(upstream);
              else onAdd(upstream);
            }}
          >
            {adding ? "Adding" : "Add and continue"}
          </Button>
          <Button variant="secondary" size="sm" onClick={() => setOther(null)}>
            Back
          </Button>
          <Button variant="secondary" size="sm" onClick={onCancel}>
            Cancel
          </Button>
        </div>
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-4" data-testid="provider-panel">
      <p className="m-0 max-w-prose text-base text-secondary">
        Log in with a subscription instead of an API key. The upstream is added; you confirm the
        provider's terms in its row.
      </p>
      {providers.length === 0 && (
        <p className="m-0 text-base text-secondary">
          The server has no provider built in. Add one with its own addresses.
        </p>
      )}
      {providers.map((provider) => (
        <div
          key={provider.name}
          className="flex flex-wrap items-center justify-between gap-4 rounded-control bg-surface p-3"
        >
          <div className="flex min-w-0 flex-col gap-1">
            <span className="min-w-0 break-words text-base font-semibold text-text">
              {provider.title}
            </span>
            <span className="min-w-0 break-all font-mono text-sm text-secondary">
              {provider.protocol} · {provider.base_url}
            </span>
            {provider.needs && (
              <span className="text-sm text-attention-text">
                Needs {provider.needs}, which is not served yet.
              </span>
            )}
          </div>
          <Button
            variant="secondary"
            size="sm"
            disabled={adding}
            title={adding ? "A provider is being added" : undefined}
            onClick={() => onAdd(presetUpstream(provider))}
          >
            Log in
          </Button>
        </div>
      ))}
      <div className="flex flex-wrap items-center justify-between gap-4 rounded-control bg-surface p-3">
        <div className="flex min-w-0 flex-col gap-1">
          <span className="text-base font-semibold text-text">Other</span>
          <span className="min-w-0 break-words text-sm text-secondary">
            Any provider with an OAuth login: enter its addresses.
          </span>
        </div>
        <Button
          variant="secondary"
          size="sm"
          aria-label="Log in to Other"
          onClick={() => setOther(blankOther())}
        >
          Log in
        </Button>
      </div>
      {problem && <Notice tone="error">{problem}</Notice>}
      <div>
        <Button variant="secondary" size="sm" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  );
}
