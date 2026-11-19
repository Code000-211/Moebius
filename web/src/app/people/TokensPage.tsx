/**
 * A person's own tokens: the ones they sign in to the command line and other machines with. A new one is named and
 * shown once; any of them can be revoked, which ends the sessions it opened. Every role has this page.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { newOwnToken, ownTokens } from "../../api/calls/people";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { IssuedToken } from "../../api/types/server";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { IssuedNotice, TokensList } from "./TokensList";

export function TokensPage() {
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const [issued, setIssued] = useState<IssuedToken | null>(null);
  const make = useMutation({
    mutationFn: newOwnToken,
    onSuccess: async (made) => {
      setIssued(made);
      setName("");
      await queryClient.invalidateQueries({ queryKey: keys.ownTokens });
    },
  });
  return (
    <div data-testid="tokens-page" className="flex flex-col gap-6">
      <PageHeader
        title="My tokens"
        context="The tokens you sign in with from the command line and other machines. Revoke one you no longer use."
      />
      <Box title="Make a token">
        <form
          className="flex flex-col gap-3"
          onSubmit={(event) => {
            event.preventDefault();
            make.mutate(name.trim());
          }}
        >
          <Field label="Name" hint="Where it is used, such as laptop.">
            <Input value={name} onChange={(event) => setName(event.target.value)} />
          </Field>
          {make.isError && (
            <Notice tone="error">
              {sayError(make.error, "Could not make the token. Try again.")}
            </Notice>
          )}
          {issued && <IssuedNotice issued={issued} />}
          <Button type="submit" disabled={name.trim() === "" || make.isPending}>
            Make token
          </Button>
        </form>
      </Box>
      <Box title="Tokens">
        <TokensList queryKey={keys.ownTokens} read={ownTokens} testId="own-tokens" />
      </Box>
    </div>
  );
}
