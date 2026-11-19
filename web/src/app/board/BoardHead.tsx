/**
 * The head of the board page and what stands under it: the layout switch, and the notice for a repository that still has a
 * `.moebius` folder. Both are drawn by `BoardPage.tsx`.
 */
import { KanbanIcon, ListBulletsIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { readSettings } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Notice } from "../../ui/page/Notice";

/**
 * Said on the board of a repository whose default branch still has a `.moebius/` folder: nothing in it is read any
 * more, so its rules, checks and scripts do not apply until they are moved into the project's settings.
 */
export function LeftoverNotice({ repository }: { repository: number }) {
  const settings = useQuery({
    queryKey: keys.projectSettings(repository),
    queryFn: () => readSettings(repository),
  });
  const leftover = settings.data?.leftover ?? [];
  if (leftover.length === 0) return null;
  return (
    <Notice
      tone="error"
      className="mb-4"
      action={
        <Link className="underline" to={`/projects/${repository}`}>
          Project settings
        </Link>
      }
    >
      <span className="font-semibold">This repository still has a .moebius folder. </span>
      Nothing in it is read any more, so its rules, checks and scripts do not apply (
      {leftover.join(", ")}). Move them into the project's settings.
    </Notice>
  );
}

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
