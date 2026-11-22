/**
 * A person's own tokens: the ones they sign in to the command line and other machines with. A new one is named and
 * shown once; any of them can be revoked, which ends the sessions it opened. Every role has this page.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { newOwnToken, ownTokens } from "../../api/calls/people";
import { keys } from "../../api/keys";
import type { IssuedToken } from "../../api/types/server";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { useErrorToast } from "../../ui/lib/useErrorToast";
import { Box } from "../../ui/page/Box";
import { FieldRow, Rows } from "../../ui/page/Field";
import { AreaTitle } from "../settings/parts";
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
  useErrorToast(make.error, "Could not make the token.");
  return (
    <div data-testid="tokens-page" className="flex max-w-3xl flex-col gap-6">
      <AreaTitle title="My tokens" />
      <Box title="Make a token">
        <form
          className="flex flex-col gap-3"
          onSubmit={(event) => {
            event.preventDefault();
            make.mutate(name.trim());
          }}
        >
          <Rows className="border-t-0">
            <FieldRow wide label="Name" description="Where it is used, such as laptop.">
              <Input value={name} onChange={(event) => setName(event.target.value)} />
            </FieldRow>
          </Rows>
          {issued && <IssuedNotice issued={issued} />}
          <Button
            type="submit"
            size="sm"
            className="self-start"
            disabled={name.trim() === "" || make.isPending}
          >
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
