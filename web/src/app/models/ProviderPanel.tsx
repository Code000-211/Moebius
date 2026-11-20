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
import { FieldRow, Rows, Select } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { Section } from "../settings/parts";
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
      <Section title="A provider with an OAuth login">
        <div data-testid="provider-panel">
          <Rows>
            <FieldRow label="Protocol" wide>
              <Select
                options={[...PROTOCOLS]}
                value={other.protocol}
                onChange={(protocol) => edit({ protocol })}
              />
            </FieldRow>
            <FieldRow label="Base URL" wide>
              <Input
                mono
                value={other.baseUrl}
                placeholder="https://api.example.com"
                onChange={(event) => edit({ baseUrl: event.target.value })}
              />
            </FieldRow>
            <FieldRow label="Flow" wide>
              <Select options={[...FLOWS]} value={other.flow} onChange={(flow) => edit({ flow })} />
            </FieldRow>
            <FieldRow label="Client ID" wide>
              <Input
                mono
                value={other.clientId}
                onChange={(event) => edit({ clientId: event.target.value })}
              />
            </FieldRow>
            <FieldRow label={urlLabel} wide>
              <Input
                mono
                value={other.url}
                placeholder="https://login.example.com/authorize"
                onChange={(event) => edit({ url: event.target.value })}
              />
            </FieldRow>
            <FieldRow label="Token URL" wide>
              <Input
                mono
                value={other.tokenUrl}
                placeholder="https://login.example.com/token"
                onChange={(event) => edit({ tokenUrl: event.target.value })}
              />
            </FieldRow>
            <FieldRow label="Scopes, separated by spaces" wide>
              <Input
                mono
                value={other.scopes}
                onChange={(event) => edit({ scopes: event.target.value })}
              />
            </FieldRow>
          </Rows>
          <div className="flex flex-col gap-3 border-t border-hairline py-3">
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
        </div>
      </Section>
    );
  }
  return (
    <Section title="Log in to a provider">
      <div data-testid="provider-panel">
        <p className="m-0 mb-3 max-w-prose text-sm text-secondary">
          Log in with a subscription instead of an API key. The upstream is added; you confirm the
          provider's terms in its row.
        </p>
        {providers.length === 0 && (
          <p className="m-0 mb-3 text-sm text-secondary">
            The server has no provider built in. Add one with its own addresses.
          </p>
        )}
        <Rows>
          {providers.map((provider) => (
            <FieldRow
              key={provider.name}
              label={provider.title}
              description={
                <>
                  <span className="break-all font-mono">
                    {provider.protocol} · {provider.base_url}
                  </span>
                  {provider.needs && (
                    <span className="block text-attention-text">
                      Needs {provider.needs}, which is not served yet.
                    </span>
                  )}
                </>
              }
            >
              <Button
                variant="secondary"
                size="sm"
                disabled={adding}
                title={adding ? "A provider is being added" : undefined}
                onClick={() => onAdd(presetUpstream(provider))}
              >
                Log in
              </Button>
            </FieldRow>
          ))}
          <FieldRow
            label="Other"
            description="Any provider with an OAuth login: enter its addresses."
          >
            <Button
              variant="secondary"
              size="sm"
              aria-label="Log in to Other"
              onClick={() => setOther(blankOther())}
            >
              Log in
            </Button>
          </FieldRow>
        </Rows>
        {problem && (
          <div className="pt-3">
            <Notice tone="error">{problem}</Notice>
          </div>
        )}
        <div className="border-t border-hairline py-3">
          <Button variant="secondary" size="sm" onClick={onCancel}>
            Cancel
          </Button>
        </div>
      </div>
    </Section>
  );
}
