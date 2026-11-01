/**
 * "Log in to a provider" on the Models page (the desktop app's `provider`): the providers the server has built in
 * (`GET /v1/models/providers`) and "Other" with the fields of an `oauth` table. Choosing one adds the upstream through
 * `PUT /v1/settings` (live, no restart); the row of the new upstream then asks for the confirmation of the warning and
 * logs in.
 */
import { useState } from "react";
import type { Provider } from "../../api/types/models";
import { Button } from "../../ui/components/Button";
import { Field, Notice, Select, TextInput } from "../harness/form";
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
      <div className="flex flex-col gap-plusfifty" data-testid="provider-panel">
        <div className="grid gap-plusfifty sm:grid-cols-[1fr_3fr]">
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
              <TextInput
                id={id}
                mono
                value={other.baseUrl}
                placeholder="https://api.example.com"
                onChange={(event) => edit({ baseUrl: event.target.value })}
              />
            )}
          </Field>
        </div>
        <div className="grid gap-plusfifty sm:grid-cols-[1fr_3fr]">
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
              <TextInput
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
            <TextInput
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
            <TextInput
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
            <TextInput
              id={id}
              mono
              value={other.scopes}
              onChange={(event) => edit({ scopes: event.target.value })}
            />
          )}
        </Field>
        {(missing ?? problem) && <Notice tone="error">{missing ?? problem}</Notice>}
        <div className="flex gap-plusfifty">
          <Button
            size="sm"
            disabled={adding}
            onClick={() => {
              const upstream = otherUpstream(other, Math.floor(Date.now() / 1000));
              if (typeof upstream === "string") setMissing(upstream);
              else onAdd(upstream);
            }}
          >
            {adding ? "Adding…" : "Add and continue"}
          </Button>
          <Button variant="outline" size="sm" onClick={() => setOther(null)}>
            Back
          </Button>
          <Button variant="outline" size="sm" onClick={onCancel}>
            Cancel
          </Button>
        </div>
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-plusfifty" data-testid="provider-panel">
      <p className="m-0 text-sm text-normal">
        Log in with a subscription instead of an API key. The upstream is added; you confirm the
        provider's terms in its row.
      </p>
      {providers.map((provider) => (
        <div key={provider.name} className="flex items-center justify-between gap-plusfifty">
          <div className="min-w-0">
            <div className="text-sm font-semibold text-high">{provider.title}</div>
            <div className="truncate font-mono text-[11.5px] text-low">
              {provider.protocol} · {provider.base_url}
            </div>
            {provider.needs && (
              <div className="text-xs text-amber">
                Needs {provider.needs}, which is not served yet.
              </div>
            )}
          </div>
          <Button
            variant="outline"
            size="sm"
            disabled={adding}
            onClick={() => onAdd(presetUpstream(provider))}
          >
            Log in
          </Button>
        </div>
      ))}
      <div className="flex items-center justify-between gap-plusfifty">
        <div>
          <div className="text-sm font-semibold text-high">Other</div>
          <div className="text-[11.5px] text-low">
            Any provider with an OAuth login: enter its addresses.
          </div>
        </div>
        <Button
          variant="outline"
          size="sm"
          aria-label="Log in to Other"
          onClick={() => setOther(blankOther())}
        >
          Log in
        </Button>
      </div>
      {problem && <Notice tone="error">{problem}</Notice>}
      <div>
        <Button variant="outline" size="sm" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  );
}
