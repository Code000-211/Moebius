/**
 * The Milestone property of a card, in the page's right column: the milestones the card is in, and a menu that puts it
 * in one or takes it out, saved at once (`ui/page/EditableProperty.tsx`). The server answers only for a card of a
 * project the person may see.
 */
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { cardMilestones, listMilestones, setCardMilestones } from "../../api/calls/milestones";
import { keys } from "../../api/keys";
import { EditableProperty } from "../../ui/page/EditableProperty";

export function MilestonesProperty({
  card,
  onSaved,
}: {
  card: number;
  /** Called after a save, refused or not, so what the page shows is what the server has. */
  onSaved: () => void;
}) {
  const queryClient = useQueryClient();
  const held = useQuery({
    queryKey: keys.cardMilestones(card),
    queryFn: () => cardMilestones(card),
  });
  const all = useQuery({ queryKey: keys.milestones, queryFn: listMilestones });
  const ids = (held.data ?? []).map((m) => String(m.id));
  return (
    <EditableProperty
      name="Milestone"
      editor={{
        kind: "people",
        people: (all.data ?? []).map((m): [string, string] => [String(m.id), m.name]),
        value: ids,
        onSave: async (next) => {
          try {
            // The answer is the card's milestones: shown at once.
            queryClient.setQueryData(
              keys.cardMilestones(card),
              await setCardMilestones(card, next.map(Number)),
            );
          } catch (error) {
            void queryClient.invalidateQueries({ queryKey: keys.cardMilestones(card) });
            throw error;
          } finally {
            onSaved();
          }
        },
      }}
    >
      {held.data === undefined || held.data.length === 0 ? (
        <span className="text-secondary">None</span>
      ) : (
        <span className="flex min-w-0 flex-col gap-0.5 py-0.5">
          {held.data.map((m) => (
            <span key={m.id} className="truncate">
              {m.name}
            </span>
          ))}
        </span>
      )}
    </EditableProperty>
  );
}
