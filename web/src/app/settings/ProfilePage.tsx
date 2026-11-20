/**
 * The Profile entry of Settings: whom this browser is signed in as (name, handle, role) and the way out of the session.
 * Signing out does something rather than going somewhere, so it is the last row of this page.
 */
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { getSession } from "../../api/calls/session";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { FieldRow, Rows } from "../../ui/page/Field";
import { leaveSession } from "../shell/session";
import { AreaTitle, ValueRow } from "./parts";

export function ProfilePage() {
  const queryClient = useQueryClient();
  const session = useQuery({ queryKey: keys.session, queryFn: getSession, enabled: false });
  const [leaving, setLeaving] = useState(false);
  const person = session.data?.person;
  return (
    <div className="max-w-3xl">
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
    </div>
  );
}
