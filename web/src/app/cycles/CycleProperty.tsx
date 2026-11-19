/**
 * The Cycle property of a card, in the page's right column: the open cycle of the card's project the card is in, and a
 * menu that puts it in another or takes it out, saved at once (`ui/page/EditableProperty.tsx`). A card is in at most
 * one open cycle, and only of its own project's, so the menu lists just those.
 */
import { useQuery } from "@tanstack/react-query";
import { cardCycle, listCycles, setCardCycle } from "../../api/calls/cycles";
import { keys } from "../../api/keys";
import { EditableProperty } from "../../ui/page/EditableProperty";

export function CycleProperty({
  card,
  project,
  onSaved,
}: {
  card: number;
  /** The project the card is in. */
  project: number;
  /** Called after a save, refused or not, so what the page shows is what the server has. */
  onSaved: () => void;
}) {
  const held = useQuery({ queryKey: keys.cardCycle(card), queryFn: () => cardCycle(card) });
  const all = useQuery({
    queryKey: keys.projectCycles(project),
    queryFn: () => listCycles(project),
  });
  const open = (all.data ?? []).filter((c) => c.closed_at === null);
  return (
    <EditableProperty
      name="Cycle"
      editor={{
        kind: "person",
        people: open.map((c): [string, string] => [String(c.id), c.name]),
        value: held.data ? String(held.data.id) : null,
        none: "No cycle",
        onSave: async (next) => {
          try {
            await setCardCycle(card, next === null ? null : Number(next));
          } finally {
            onSaved();
          }
        },
      }}
    >
      {held.data ? (
        <span className="truncate">{held.data.name}</span>
      ) : (
        <span className="text-secondary">None</span>
      )}
    </EditableProperty>
  );
}
