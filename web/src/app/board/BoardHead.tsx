/**
 * The head of the board page and what stands under it: the layout switch, and the notice for a repository that still has a
 * `.moebius` folder. Both are drawn by `BoardPage.tsx`.
 */
import { InfoIcon, KanbanIcon, ListBulletsIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { memo } from "react";
import { Link } from "react-router";
import { readSettings } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";

/**
 * Said on the board of a repository whose default branch still has a `.moebius/` folder: nothing in it is read any
 * more, so its rules, checks and scripts do not apply until they are moved into the project's settings.
 */
export const LeftoverNotice = memo(function LeftoverNotice({ repository }: { repository: number }) {
  const settings = useQuery({
    queryKey: keys.projectSettings(repository),
    queryFn: () => readSettings(repository),
  });
  const leftover = settings.data?.leftover ?? [];
  if (leftover.length === 0) return null;
  return (
    // A quiet line: the standard allows no red banner, and a leftover folder stops nothing.
    <p
      role="status"
      className="m-0 mb-4 flex min-h-9 flex-wrap items-center gap-2 border-b border-hairline px-1 py-1.5 text-sm text-secondary"
    >
      <InfoIcon className="size-4 shrink-0" aria-hidden="true" />
      <span className="min-w-0 flex-1 break-words">
        <span className="text-text">This repository still has a .moebius folder. </span>
        Nothing in it is read any more, so its rules, checks and scripts do not apply (
        {leftover.join(", ")}). Move them into the project's settings.
      </span>
      <Link
        className="shrink-0 text-secondary underline hover:text-text"
        to={`/projects/${repository}`}
      >
        Project settings
      </Link>
    </p>
  );
});

/** The two layouts as two buttons in the head of the page; Display holds the same choice among the rest. */
export function LayoutSwitch({
  layout,
  onChange,
}: {
  layout: "board" | "list";
  onChange: (layout: "board" | "list") => void;
}) {
  const choices = [
    ["board", "Board", KanbanIcon],
    ["list", "List", ListBulletsIcon],
  ] as const;
  return (
    <fieldset aria-label="Layout" className="m-0 flex min-w-0 items-center gap-0.5 border-0 p-0">
      {choices.map(([value, label, Icon]) => (
        <Button
          key={value}
          variant={layout === value ? "secondary" : "ghost"}
          size="sm"
          aria-pressed={layout === value}
          aria-label={`${label} layout`}
          title={`${label} layout`}
          onClick={() => onChange(value)}
        >
          <Icon className="size-4" aria-hidden="true" />
        </Button>
      ))}
    </fieldset>
  );
}
