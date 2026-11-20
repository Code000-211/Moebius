/**
 * The webhooks of the forges the server is set up for: where each forge sends to and the secret it signs with, which
 * is shown only when asked for. A forge that can reach the server tells it at once that something changed. Only a
 * person may read the secrets, so the server's refusal of an agent's token is shown as it came.
 *
 * Each forge is a group of rows: its name and whether webhooks are on for it, then the address and the secret it signs
 * with as label-left rows, then the controls, all at the height of every other control on the page.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { makeWebhookSecret, webhooks } from "../../api/calls/settings";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Webhook } from "../../api/types/settings";
import { Button } from "../../ui/components/Button";
import { FieldRow, Rows } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { Code, CopyValue, Section, ValueRow } from "./parts";

/** The name a forge is called by on the page. */
const FORGES: Record<string, string> = { github: "GitHub", gitlab: "GitLab", forgejo: "Forgejo" };

const forgeName = (forge: string): string =>
  FORGES[forge] ?? forge.charAt(0).toUpperCase() + forge.slice(1);

/** The secret as the page shows it. */
export function secretText(webhook: Webhook, shown: boolean): string {
  if (webhook.secret === null) return "No secret yet: webhooks of this forge are off";
  return shown ? webhook.secret : "••••••••••••";
}

export function WebhooksBox() {
  const queryClient = useQueryClient();
  const list = useQuery({ queryKey: keys.webhooks, queryFn: () => webhooks() });
  const [shown, setShown] = useState<ReadonlySet<string>>(new Set());
  const make = useMutation({
    mutationFn: makeWebhookSecret,
    onSuccess: (made) =>
      queryClient.setQueryData<Webhook[]>(keys.webhooks, (current) =>
        (current ?? []).map((each) => (each.forge === made.forge ? { ...each, ...made } : each)),
      ),
  });
  if (list.isError) {
    return (
      <Section title="Webhooks">
        <Notice tone="error">
          {sayError(list.error, "Could not read the webhooks. Try again.")}
        </Notice>
      </Section>
    );
  }
  if (!list.data || list.data.length === 0) return null;
  const toggle = (forge: string) =>
    setShown((current) => {
      const next = new Set(current);
      if (!next.delete(forge)) next.add(forge);
      return next;
    });
  return (
    <Section title="Webhooks">
      <div className="flex flex-col gap-6">
        <p className="m-0 max-w-prose text-sm text-secondary">
          A forge that can reach the server tells it at once that something changed. Set the URL and
          the secret in the forge's webhook settings, with JSON content; the server still polls.
        </p>
        <div className="flex flex-col gap-8">
          {list.data.map((webhook) => (
            <ForgeCard
              key={webhook.forge}
              webhook={webhook}
              shown={shown.has(webhook.forge)}
              busy={make.isPending}
              error={
                make.isError && make.variables === webhook.forge
                  ? sayError(make.error, "Could not make a secret. Try again.")
                  : null
              }
              onToggle={() => toggle(webhook.forge)}
              onMake={() => {
                make.mutate(webhook.forge);
              }}
            />
          ))}
        </div>
      </div>
    </Section>
  );
}

/** One forge: where it sends, what it signs with, and the controls over the secret. */
function ForgeCard({
  webhook,
  shown,
  busy,
  error,
  onToggle,
  onMake,
}: {
  webhook: Webhook;
  shown: boolean;
  busy: boolean;
  error: string | null;
  onToggle: () => void;
  onMake: () => void;
}) {
  const name = forgeName(webhook.forge);
  const secret = `${window.location.origin}${webhook.path}`;
  return (
    <section data-testid={`webhook-${webhook.forge}`} className="flex min-w-0 flex-col gap-3">
      <header className="flex flex-wrap items-baseline gap-2">
        <h4 className="m-0 min-w-0 flex-1 break-words text-base font-medium text-text">{name}</h4>
        <span className="shrink-0 text-sm text-secondary">
          {webhook.secret === null ? "Webhooks off" : "Webhooks on"}
        </span>
      </header>
      <Rows>
        <ValueRow label="Sends to" value={<Code>{secret}</Code>} />
        <ValueRow
          label="Secret"
          value={
            webhook.secret === null ? (
              secretText(webhook, shown)
            ) : (
              <Code>{secretText(webhook, shown)}</Code>
            )
          }
        />
        <ValueRow label="Sent in" value={<Code>{webhook.header}</Code>} />
        <FieldRow label="Secret actions">
          <div className="flex flex-wrap items-center justify-end gap-2">
            {webhook.secret !== null && (
              <>
                <Button size="sm" variant="secondary" onClick={onToggle}>
                  {shown ? "Hide" : "Show"}
                </Button>
                <CopyValue text={webhook.secret} label={`Copy the secret of ${webhook.forge}`} />
              </>
            )}
            <Button size="sm" variant="secondary" disabled={busy} onClick={onMake}>
              {webhook.secret !== null ? "Make a new secret" : "Make a secret"}
            </Button>
          </div>
        </FieldRow>
      </Rows>
      {error && <Notice tone="error">{error}</Notice>}
    </section>
  );
}
