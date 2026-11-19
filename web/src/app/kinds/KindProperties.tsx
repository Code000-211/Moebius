/**
 * The Kind property of a card and one property for each field of its kind, in the page's right column, each edited in
 * place and saved alone (`ui/page/EditableProperty.tsx`). The kinds on offer are those the card's project picked; a card
 * keeps a kind its project stopped offering until someone changes it. A field is edited as its type says: text and number
 * in a field, a date in a date field, a choice or a person from a menu; clearing a value saves it as none. Nothing is
 * drawn for a card of a project that offers no kind and has none.
 */
import { useQuery } from "@tanstack/react-query";
import { listKinds, setCardFields, setCardKind } from "../../api/calls/kinds";
import { people as loadPeople } from "../../api/calls/people";
import { keys } from "../../api/keys";
import type { Card } from "../../api/types/board";
import type { KindField } from "../../api/types/kinds";
import { EditableProperty } from "../../ui/page/EditableProperty";
import { assignable } from "../people/AssigneesProperty";

const NONE = <span className="text-secondary">None</span>;

export function KindProperties({
  card,
  saved,
}: {
  card: Card;
  /** Sends a save and reads the board again once the server answered. */
  saved: (request: Promise<unknown>) => Promise<void>;
}) {
  const kinds = useQuery({ queryKey: keys.issueKinds, queryFn: listKinds });
  const people = useQuery({ queryKey: keys.people, queryFn: loadPeople });
  const all = kinds.data ?? [];
  const offered = all.filter(
    (kind) => card.project_id !== undefined && kind.projects.includes(card.project_id),
  );
  const own = all.find((kind) => kind.name === card.kind);
  if (offered.length === 0 && !own) return null;
  const choices: Array<[string, string]> = [
    ["", "None"],
    ...offered.map((kind): [string, string] => [kind.name, kind.name]),
  ];
  if (own && !offered.includes(own)) choices.push([own.name, own.name]);
  return (
    <>
      <EditableProperty
        name="Kind"
        editor={{
          kind: "choice",
          options: choices,
          value: card.kind ?? "",
          onSave: (next) => saved(setCardKind(card.id, next === "" ? null : next)),
        }}
      >
        {card.kind ?? NONE}
      </EditableProperty>
      {own?.fields.map((field) => (
        <FieldProperty
          key={field.id}
          field={field}
          value={card.fields?.[field.name]}
          people={assignable(people.data)}
          onSave={(value) => saved(setCardFields(card.id, { [field.name]: value }))}
        />
      ))}
    </>
  );
}

function FieldProperty({
  field,
  value,
  people,
  onSave,
}: {
  field: KindField;
  value: string | number | undefined;
  people: Array<[string, string]>;
  onSave: (value: string | number | null) => Promise<unknown>;
}) {
  const shown = value === undefined ? NONE : String(value);
  switch (field.type) {
    case "choice":
      return (
        <EditableProperty
          name={field.name}
          editor={{
            kind: "choice",
            options: [["", "None"], ...field.options.map((o): [string, string] => [o, o])],
            value: value === undefined ? "" : String(value),
            onSave: (next) => onSave(next === "" ? null : next),
          }}
        >
          {shown}
        </EditableProperty>
      );
    case "person": {
      const handle = value === undefined ? null : String(value);
      const offered =
        people.some(([h]) => h === handle) || handle === null
          ? people
          : [...people, [handle, `@${handle}`] as [string, string]];
      return (
        <EditableProperty
          name={field.name}
          editor={{
            kind: "person",
            people: offered,
            none: "No one",
            value: handle,
            onSave,
          }}
        >
          {handle === null ? NONE : `@${handle}`}
        </EditableProperty>
      );
    }
    case "date":
      return (
        <EditableProperty
          name={field.name}
          editor={{
            kind: "date",
            value: value === undefined ? null : String(value),
            onSave,
          }}
        >
          {shown}
        </EditableProperty>
      );
    case "number":
      return (
        <EditableProperty
          name={field.name}
          editor={{
            kind: "number",
            value: typeof value === "number" ? value : null,
            onSave,
          }}
        >
          {shown}
        </EditableProperty>
      );
    default:
      return (
        <EditableProperty
          name={field.name}
          editor={{
            kind: "text",
            value: value === undefined ? "" : String(value),
            onSave: (next) => onSave(next.trim() === "" ? null : next),
          }}
        >
          {shown}
        </EditableProperty>
      );
  }
}
