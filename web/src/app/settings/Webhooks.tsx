/**
 * The webhooks of the forges the server is set up for: where each forge sends to and the secret it signs with, which
 * is shown only when asked for. A forge that can reach the server tells it at once that something changed. Only a
 * person may read the secrets, so the server's refusal of an agent's token is shown as it came.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { makeWebhookSecret, webhooks } from "../../api/calls/settings";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Webhook } from "../../api/types/settings";
import { CopyButton } from "../../ui/chat/CopyButton";
import { Button } from "../../ui/components/Button";
import { Box } from "../../ui/page/Box";
import { Notice } from "../../ui/page/Notice";

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
        (current ?? []).map((each) => (each.forge === made.forge ? made : each)),
      ),
  });
  if (list.isError) {
    return (
      <Box title="Webhooks">
        <Notice tone="error">{list.error.message}</Notice>
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
      <p className="m-0 mb-[12px] text-[13px] text-normal">
        A forge that can reach the server tells it at once that something changed. Set the URL and
        the secret in the forge's webhook settings, with JSON content; the server still polls.
      </p>
      <div className="flex flex-col gap-[16px]">
        {list.data.map((webhook) => {
          const visible = shown.has(webhook.forge);
          return (
            <section
              key={webhook.forge}
              data-testid={`webhook-${webhook.forge}`}
              className="flex flex-col gap-base"
            >
              <h3 className="m-0 text-[13px] font-bold text-high">{webhook.forge}</h3>
              <dl className="m-0 grid gap-x-[16px] gap-y-half [grid-template-columns:auto_1fr]">
                <dt className="font-mono text-[11px] text-low">URL</dt>
                <dd className="m-0 break-all font-mono text-[12px] text-high">
                  {`${window.location.origin}${webhook.path}`}
                </dd>
                <dt className="font-mono text-[11px] text-low">Secret</dt>
                <dd className="m-0 break-all font-mono text-[12px] text-high">
                  {secretText(webhook, visible)}
                </dd>
                <dt className="font-mono text-[11px] text-low">Sent in</dt>
                <dd className="m-0 text-[12.5px] text-high">{webhook.header}</dd>
              </dl>
              <div className="flex flex-wrap items-center gap-base">
                {webhook.secret !== null && (
                  <>
                    <Button size="sm" variant="outline" onClick={() => toggle(webhook.forge)}>
                      {visible ? "Hide" : "Show"}
                    </Button>
                    <CopyButton
                      text={webhook.secret}
                      label={`Copy the secret of ${webhook.forge}`}
                    />
                  </>
                )}
                <Button
                  size="sm"
                  variant="outline"
                  disabled={make.isPending}
                  onClick={() => make.mutate(webhook.forge)}
                >
                  {webhook.secret !== null ? "Make a new secret" : "Make a secret"}
                </Button>
              </div>
            </section>
          );
        })}
      </div>
      {make.isError && (
        <div className="mt-base">
          <Notice tone="error">
            {isApiError(make.error) ? make.error.message : "The secret could not be made."}
          </Notice>
        </div>
      )}
    </Box>
  );
}
