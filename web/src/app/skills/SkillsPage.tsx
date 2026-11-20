/**
 * The Skills page: the skills an agent can name, as one list of 36px rows, built in ones first. A row opens the skill
 * in the editor window (`SkillEditor`, `ui/components/EditorWindow.tsx`): its description, the text of `SKILL.md` and
 * its other files; "New skill" opens the same window empty. An edit of `SKILL.md` is `PUT /v1/skills/{name}`, which the
 * server makes a commit in the harness configuration repository. The skills Moebius brings are shown and cannot be
 * changed. A server without a harness configuration repository lists no skills, and the page then offers to set one
 * up. The skill open is in the address (`?skill=`, or `?new=1`), so a link opens the window.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useSearchParams } from "react-router";
import { skill as readSkill, skillList } from "../../api/calls/harness";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { Reading, SkeletonList } from "../../ui/page/Skeleton";
import { useToast } from "../../ui/page/Toast";
import { HarnessSetup, WHY_MUTED } from "../harness/HarnessSetup";
import { ImportPanel } from "../harness/ImportPanel";
import { SkillEditor } from "./SkillEditor";

export function SkillsPage() {
  const [params, setParams] = useSearchParams();
  const [importing, setImporting] = useState(false);
  const skills = useQuery({ queryKey: keys.skills, queryFn: skillList });
  const chosen = params.get("skill");
  const creating = params.get("new") === "1";
  const open = (skill: string | null, fresh = false) =>
    setParams(
      (current) => {
        const next = new URLSearchParams(current);
        if (skill === null) next.delete("skill");
        else next.set("skill", skill);
        if (fresh) next.set("new", "1");
        else next.delete("new");
        return next;
      },
      { replace: true },
    );
  const close = () => open(null);

  // A server with a harness configuration repository always lists the built-in skills, so none means there is none.
  const editable = skills.isSuccess && skills.data.length > 0;
  const noRepository = skills.isSuccess && skills.data.length === 0;
  // Why the two buttons that change skills are quiet. It is printed beside them, so that a keyboard and a screen
  // reader reach it too: a `disabled` button is out of the tab order and carries no tooltip there.
  const whyQuiet = editable
    ? undefined
    : skills.isError
      ? "The skills could not be read, so none can be added yet."
      : skills.isPending
        ? undefined
        : WHY_MUTED;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <PageHeader
        title="Skills"
        context="What an agent reads before it starts."
        actions={
          <div className="flex flex-wrap items-center gap-3">
            {whyQuiet && <span className="text-sm text-secondary">{whyQuiet}</span>}
            <Button
              variant="secondary"
              size="sm"
              disabled={!editable}
              onClick={() => setImporting(true)}
            >
              Import
            </Button>
            <Button size="sm" disabled={!editable} onClick={() => open(null, true)}>
              <PlusIcon className="size-4" weight="bold" />
              New skill
            </Button>
          </div>
        }
      />
      {skills.isPending && (
        <Reading message="Reading the skills">
          <SkeletonList rows={4} message="Reading the skills" />
        </Reading>
      )}
      {skills.isError && (
        <Notice
          tone="error"
          action={
            <Button
              size="sm"
              variant="secondary"
              disabled={skills.isFetching}
              onClick={() => void skills.refetch()}
            >
              Try again
            </Button>
          }
        >
          {`Could not read the skills. ${skills.error.message}`}
        </Notice>
      )}
      {noRepository && <HarnessSetup />}
      {editable && (
        <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
          {importing && <ImportPanel kind="skills" onClose={() => setImporting(false)} />}
          {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely */}
          <ul role="list" className="m-0 flex list-none flex-col p-0">
            {skills.data.map((skill) => (
              <li key={skill.name}>
                <button
                  type="button"
                  data-testid="skill-row"
                  aria-label={`Edit skill ${skill.name}`}
                  onClick={() => open(skill.name)}
                  className="flex h-9 w-full cursor-pointer items-center gap-2.5 border-b border-hairline px-4 text-left text-base text-text hover:bg-row-hover"
                >
                  <span
                    className="shrink-0 truncate font-mono text-sm font-medium"
                    style={{ maxWidth: "14rem" }}
                  >
                    {skill.name}
                  </span>
                  <span className="min-w-0 flex-1 truncate text-muted">{skill.description}</span>
                  {skill.built_in && <span className="shrink-0 text-xs text-muted">Built in</span>}
                </button>
              </li>
            ))}
          </ul>
          {creating ? (
            <SkillEditor
              key="new"
              name=""
              naming
              builtIn={false}
              source={undefined}
              onClose={close}
            />
          ) : chosen !== null ? (
            <ChosenSkill
              key={chosen}
              name={chosen}
              builtIn={skills.data.find((skill) => skill.name === chosen)?.built_in ?? false}
              onClose={close}
            />
          ) : null}
        </div>
      )}
    </div>
  );
}

/** Reads the files of the skill `name`, and opens the window on them. */
function ChosenSkill({
  name,
  builtIn,
  onClose,
}: {
  name: string;
  builtIn: boolean;
  onClose: () => void;
}) {
  const toast = useToast();
  const detail = useQuery({
    queryKey: keys.skill(name),
    queryFn: () => readSkill(name),
    retry: false,
  });
  useEffect(() => {
    if (!detail.isError) return;
    toast({ kind: "error", text: `Could not read ${name}. ${detail.error.message}` });
    onClose();
  }, [detail.isError, detail.error, name, toast, onClose]);
  if (!detail.data) return null;
  return (
    <SkillEditor
      name={name}
      naming={false}
      builtIn={builtIn}
      source={detail.data.source}
      detail={detail.data}
      onClose={onClose}
    />
  );
}
