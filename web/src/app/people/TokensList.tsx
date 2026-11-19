/**
 * A list of access tokens with when each was last used and a way to revoke it, shared by the People page (a person's
 * tokens, read by an owner) and the page of a person's own tokens. A token is never shown again after it is made, so a
 * newly made one is shown once, with a button that copies it.
 */
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { revokeToken } from "../../api/calls/people";
import { sayError } from "../../api/errorText";
import type { AccessToken, IssuedToken } from "../../api/types/server";
import { CopyButton } from "../../ui/chat/CopyButton";
import { Button } from "../../ui/components/Button";
import { ago } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { Notice } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";

/** What a token's row says about its use. */
export function usedText(token: AccessToken, now: number): string {
  if (token.revoked_at !== null) return "Revoked";
  return token.last_used_at === null ? "Never used" : `Used ${ago(now, token.last_used_at)}`;
}

/** The token just made, to copy once. */
export function IssuedNotice({ issued }: { issued: IssuedToken }) {
  return (
    <div data-testid="issued-token" className="flex flex-col gap-2 rounded-card bg-sunken p-3">
      <p className="m-0 text-base text-text">
        The token <span className="font-semibold">{issued.name}</span> is shown only now. Copy it
        and hand it over.
      </p>
      <span className="flex flex-wrap items-center gap-2">
        <code className="break-all font-mono text-sm text-secondary">{issued.token}</code>
        <CopyButton text={issued.token} label={`Copy token ${issued.name}`} />
      </span>
    </div>
  );
}

export function TokensList({
  queryKey,
  read,
  testId,
}: {
  queryKey: readonly unknown[];
  read: () => Promise<AccessToken[]>;
  testId: string;
}) {
  const queryClient = useQueryClient();
  const now = useNow();
  const list = useQuery({ queryKey, queryFn: read });
  const [failed, setFailed] = useState<string | null>(null);
  if (list.isPending) return <SkeletonLines lines={2} message="Reading the tokens" />;
  if (list.isError) {
    return (
      <Notice tone="error">{sayError(list.error, "Could not read the tokens. Try again.")}</Notice>
    );
  }
  const revoke = async (token: AccessToken) => {
    setFailed(null);
    try {
      await revokeToken(token.id);
      await queryClient.invalidateQueries({ queryKey });
    } catch (error) {
      setFailed(sayError(error, "Could not revoke the token. Try again."));
    }
  };
  return (
    <div data-testid={testId} className="flex flex-col gap-2">
      {failed && <Notice tone="error">{failed}</Notice>}
      <ul className="m-0 flex list-none flex-col p-0">
        {list.data.map((token) => (
          <li
            key={token.id}
            data-testid="token-row"
            className="flex min-h-9 flex-wrap items-center gap-3 border-b border-hairline py-1 last:border-b-0"
          >
            <span className="min-w-0 flex-1 break-words text-base text-text">{token.name}</span>
            <span className="text-sm text-secondary">{usedText(token, now)}</span>
            {token.revoked_at === null && (
              <Button
                variant="ghost"
                size="sm"
                aria-label={`Revoke token ${token.name}`}
                onClick={() => void revoke(token)}
              >
                Revoke
              </Button>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
