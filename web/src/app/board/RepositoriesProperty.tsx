/**
 * The Repositories property of a card, in the page's right column: the repositories the card works in, its own
 * issue's first and fixed, and a menu that adds or takes off the other repositories of the card's project, saved at
 * once (`ui/page/EditableProperty.tsx`). Each repository gets a change of its own, merged in the order listed. The
 * server refuses to take off a repository in which the card has an open change, and says why under the property.
 */
import { useQuery } from "@tanstack/react-query";
import { listProjects } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import type { Board, Card } from "../../api/types/board";
import { EditableProperty } from "../../ui/page/EditableProperty";

/** The repositories a card may be given: those of its project, and the project's files, except its own. */
export function offered(
  card: Card,
  own: string,
  projects: Array<{
    id: number;
    repositories: Array<{ id: number; name: string }>;
    files: number | null;
  }>,
  boards: Board[],
): string[] {
  const project = projects.find((p) => p.id === card.project_id);
  if (!project) return [];
  const names = project.repositories.map((r) => r.name);
  const files = boards.find((b) => b.repository.id === project.files)?.repository.name;
  if (files !== undefined) names.push(files);
  return [...new Set(names)].filter((name) => name !== own);
}

export function RepositoriesProperty({
  card,
  own,
  boards,
  onSave,
}: {
  card: Card;
  /** The name of the repository of the card's own issue. */
  own: string;
  /** Every board, which names the project's files repository. */
  boards: Board[];
  /** Names the repositories besides the own one, in the order the changes are merged. */
  onSave: (repositories: string[]) => Promise<unknown>;
}) {
  const projects = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  const named = (card.repositories ?? [own]).filter((name) => name !== own);
  const options = offered(card, own, projects.data ?? [], boards);
  return (
    <EditableProperty
      name="Repositories"
      editor={{
        kind: "choices",
        options: options.map((name): [string, string] => [name, name]),
        value: named,
        fixed: [own],
        // A repository added goes last, so the changes are merged in the order the repositories were added.
        onSave,
      }}
    >
      <span className="flex min-w-0 flex-col gap-0.5 py-0.5">
        <span className="truncate">{`${own} (own)`}</span>
        {named.map((name) => (
          <span key={name} className="truncate">
            {name}
          </span>
        ))}
      </span>
    </EditableProperty>
  );
}
