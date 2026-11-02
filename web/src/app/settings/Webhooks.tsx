/**
 * The webhooks of the forges the server is set up for: where each forge sends to and the secret it signs with, which
 * is shown only when asked for. A forge that can reach the server tells it at once that something changed. Only a
 * person may read the secrets, so the server's refusal of an agent's token is shown as it came.
 *
 * Each forge is an inset card of its own: its name and whether webhooks are on for it, then the address and the secret
 * it signs with, then the controls, all at the height of every other control on the page.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { makeWebhookSecret, webhooks } from "../../api/calls/settings";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Webhook } from "../../api/types/settings";
import { Button } from "../../ui/components/Button";
import { Box } from "../../ui/page/Box";
import { Notice } from "../../ui/page/Notice";
import { Body } from "../../ui/page/PageHeader";
import { Code, CopyValue, KeyValue, KeyValueGrid } from "./parts";

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
      <Box title="Webhooks">
        <Notice tone="error">
          {sayError(list.error, "Could not read the webhooks. Try again.")}
        </Notice>
      </Box>
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
    <Box title="Webhooks">
      <div className="flex flex-col gap-6">
        <Body>
          A forge that can reach the server tells it at once that something changed. Set the URL and
          the secret in the forge's webhook settings, with JSON content; the server still polls.
        </Body>
        <div className="flex flex-col gap-4">
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
    </Box>
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
    <section
      data-testid={`webhook-${webhook.forge}`}
      className="flex flex-col gap-4 rounded-card bg-sunken p-4"
    >
      <header className="flex flex-wrap items-baseline gap-2">
        <h3 className="m-0 min-w-0 flex-1 break-words text-base font-semibold text-text">{name}</h3>
        <span className="shrink-0 text-sm text-secondary">
          {webhook.secret === null ? "Webhooks off" : "Webhooks on"}
        </span>
      </header>
      <KeyValueGrid>
        <KeyValue label="Sends to" value={<Code>{secret}</Code>} />
        <KeyValue label="Secret" value={<Code>{secretText(webhook, shown)}</Code>} />
        <KeyValue label="Sent in" value={webhook.header} />
      </KeyValueGrid>
      <div className="flex flex-wrap items-center gap-2">
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
      {error && <Notice tone="error">{error}</Notice>}
    </section>
  );
}
