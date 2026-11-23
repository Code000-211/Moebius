/**
 * The Profile entry of Settings: whom this browser is signed in as (name, handle, role), the person's logins on the forges
 * (so that taking an issue here assigns them there, ADR 0027), and the way out of the session. Signing out does something
 * rather than going somewhere, so it is the last row of this page.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { snapshot as readSnapshot } from "../../api/calls/board";
import { logins as readLogins, saveLogins } from "../../api/calls/logins";
import { getSession } from "../../api/calls/session";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Forge } from "../../api/types/board";
import type { Logins } from "../../api/types/logins";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { FieldRow, Rows } from "../../ui/page/Field";
import { forgeName } from "../board/model";
import { leaveSession } from "../shell/session";
import { AreaColumn, AreaTitle, Section, ValueRow } from "./parts";

const FORGES: Array<keyof Logins & Forge> = ["github", "gitlab", "forgejo"];

/**
 * The person's login on each forge a repository of this server is on, always GitHub's: what taking an issue here makes
 * them the assignee as. Without one nothing is assigned, and other servers may take the issue too.
 */
function ForgeLogins() {
  const queryClient = useQueryClient();
  const kept = useQuery({ queryKey: keys.logins, queryFn: readLogins });
  const boards = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  const [typed, setTyped] = useState<Partial<Logins>>({});
  const save = useMutation({
    mutationFn: (next: Logins) => saveLogins(next),
    onSuccess: (saved) => {
      setTyped({});
      queryClient.setQueryData(keys.logins, saved);
      // Who took an issue is read against the people's logins.
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      void queryClient.invalidateQueries({ queryKey: keys.start });
    },
  });
  if (!kept.data) return null;
  const used = new Set(boards.data?.boards.map((board) => board.repository.forge));
  const shown = FORGES.filter((forge) => forge === "github" || used.has(forge));
  const value = (forge: keyof Logins) => typed[forge] ?? kept.data[forge];
  const changed = shown.some((forge) => value(forge).trim() !== kept.data[forge]);
  return (
    <Section title="Your logins on forges">
      <Rows>
        {shown.map((forge, at) => {
          const name = forgeName({ forge } as Parameters<typeof forgeName>[0]) ?? forge;
          // Save stands beside the last login, in the row of the fields it saves.
          const last = at === shown.length - 1;
          return (
            <FieldRow
              key={forge}
              label={`Your login on ${name}`}
              description={
                forge === "github"
                  ? `Taking an issue here makes this login its assignee on ${name}, so that people on other servers leave it alone. Without it nothing is assigned.`
                  : undefined
              }
              wide
            >
              <div className="flex w-full items-center gap-2">
                <Input
                  aria-label={`Your login on ${name}`}
                  value={value(forge)}
                  placeholder="login, without @"
                  autoComplete="off"
                  onChange={(e) => setTyped({ ...typed, [forge]: e.target.value })}
                />
                {last && (
                  <Button
                    size="sm"
                    disabled={!changed || save.isPending}
                    onClick={() => save.mutate({ ...kept.data, ...typed })}
                  >
                    {save.isPending ? "Saving" : "Save logins"}
                  </Button>
                )}
              </div>
            </FieldRow>
          );
        })}
      </Rows>
      {save.isError && (
        <p role="alert" className="m-0 mt-2 text-sm text-error-text">
          {isApiError(save.error) ? save.error.message : "The logins could not be saved."}
        </p>
      )}
    </Section>
  );
}

export function ProfilePage() {
  const queryClient = useQueryClient();
  const session = useQuery({ queryKey: keys.session, queryFn: getSession, enabled: false });
  const [leaving, setLeaving] = useState(false);
  const person = session.data?.person;
  return (
    <AreaColumn>
      <AreaTitle title="Profile" />
      <Rows>
        {person ? (
          <>
            <ValueRow label="Name" value={person.name} />
            <ValueRow label="Handle" value={`@${person.handle}`} />
            <ValueRow label="Role" value={person.role} />
          </>
        ) : (
          <ValueRow label="Signed in as" value="The server's token, not a person" />
        )}
      </Rows>
      <ForgeLogins />
      <Rows>
        <FieldRow label="Session" description="Ends the sign-in of this browser">
          <Button
            variant="secondary"
            size="sm"
            disabled={leaving}
            onClick={() => {
              setLeaving(true);
              void leaveSession(queryClient);
            }}
          >
            {leaving ? "Signing out" : "Sign out"}
          </Button>
        </FieldRow>
      </Rows>
    </AreaColumn>
  );
}
