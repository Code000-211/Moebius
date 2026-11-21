/**
 * The Assignee property of a card or an inbox item,: the avatars and handles of the people
 * it is given to, and a menu that adds or takes off a person, saved at once (`ui/page/EditableProperty.tsx`). The
 * people on offer are those of the organisation who are not disabled; a person who was disabled after being assigned
 * stays on the list until someone takes them off.
 */
import { useQuery } from "@tanstack/react-query";
import { people as loadPeople } from "../../api/calls/people";
import { keys } from "../../api/keys";
import type { Person } from "../../api/types/server";
import { Avatar } from "../../ui/page/Avatar";
import { EditableProperty } from "../../ui/page/EditableProperty";

/** The people who can be given something: not disabled, as a handle and the name that shows them. */
export function assignable(people: Person[] | undefined): Array<[string, string]> {
  return (people ?? [])
    .filter((p) => !p.disabled)
    .map((p): [string, string] => [
      p.handle,
      p.name === "" ? `@${p.handle}` : `${p.name} (@${p.handle})`,
    ]);
}

export function AssigneesProperty({
  assignees,
  onSave,
}: {
  assignees: string[];
  onSave: (assignees: string[]) => Promise<unknown>;
}) {
  const list = useQuery({ queryKey: keys.people, queryFn: loadPeople });
  return (
    <EditableProperty
      name="Assignee"
      editor={{ kind: "people", people: assignable(list.data), value: assignees, onSave }}
    >
      {assignees.length === 0 ? (
        <span className="text-secondary">No one</span>
      ) : (
        <span className="flex min-w-0 flex-col gap-0.5 py-0.5">
          {assignees.map((handle) => (
            <span key={handle} className="flex min-w-0 items-center gap-1.5">
              <Avatar handle={handle} />
              <span className="truncate">{`@${handle}`}</span>
            </span>
          ))}
        </span>
      )}
    </EditableProperty>
  );
}
