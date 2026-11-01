/**
 * The Skills page: the skills an agent can name, and the editor of the one chosen (`SkillEditor`): its description,
 * the list of its files, and the text of `SKILL.md` or of the file open. An edit of `SKILL.md` is
 * `PUT /v1/skills/{name}`, which the server makes a commit in the harness configuration repository. The skills Moebius
 * brings are shown and cannot be changed. A server without a harness configuration repository lists no skills, and
 * the page then offers to set one up. The skill chosen is in the address (`?skill=`, or `?new=1`), and what was typed
 * for a skill is kept while the page lives, so moving along the list does not throw it away.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useSearchParams } from "react-router";
import { skill as readSkill, skillList } from "../../api/calls/harness";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Loader } from "../../ui/components/Loader";
import { Empty } from "../../ui/page/Empty";
import { chosenOrFirst, ListCard, ListDetail } from "../../ui/page/List";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { useDrafts } from "../harness/form";
import { HarnessSetup, WHY_MUTED } from "../harness/HarnessSetup";
import { ImportPanel } from "../harness/ImportPanel";
import { type SkillDraft, SkillEditor } from "./SkillEditor";

/** The draft of a new skill, which has no name to hold it yet. */
const NEW = "\u0000new";

export function SkillsPage() {
  const [params, setParams] = useSearchParams();
  const [importing, setImporting] = useState(false);
  const [saved, setSaved] = useState<{ name: string; commit: string | null } | null>(null);
  // What was typed for each skill, held while the page lives, so choosing another row keeps the draft.
  const drafts = useDrafts<SkillDraft | null>();
  const newDraft = { hold: drafts.hold(NEW), drop: () => drafts.drop(NEW) };
  const skills = useQuery({ queryKey: keys.skills, queryFn: skillList });
  const chosen = params.get("skill");
  const creating = params.get("new") === "1";
  // Opening on the first skill means the page never shows "Nothing chosen" while there is one to read.
  const picked = chosenOrFirst(chosen, creating, skills.data?.map((skill) => skill.name) ?? []);
  const select = (skill: string | null, fresh = false) =>
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
  const startNew = () => {
    setSaved(null);
    select(null, true);
  };

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
    <>
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
            <Button size="sm" disabled={!editable} onClick={startNew}>
              <PlusIcon className="size-4" weight="bold" />
              New skill
            </Button>
          </div>
        }
      />
      {skills.isPending && <Loader message="Reading the skills" className="py-8" />}
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
        <div className="flex flex-col gap-8">
          {importing && <ImportPanel kind="skills" onClose={() => setImporting(false)} />}
          <ListDetail
            list={skills.data.map((skill) => (
              <ListCard
                key={skill.name}
                title={skill.name}
                tags={skill.built_in ? [{ label: "Built in", tone: "plain" }] : []}
                line={skill.description}
                selected={!creating && picked === skill.name}
                onClick={() => {
                  setSaved(null);
                  select(skill.name);
                }}
              />
            ))}
            detail={
              creating ? (
                <SkillEditor
                  key="new"
                  name=""
                  naming
                  builtIn={false}
                  source={undefined}
                  draft={drafts.take(NEW, () => null)}
                  drafts={newDraft}
                  onNamed={(name, commit) => {
                    drafts.drop(NEW);
                    setSaved({ name, commit });
                    select(name);
                  }}
                  onDeleted={() => {
                    drafts.drop(NEW);
                    select(null);
                  }}
                />
              ) : picked !== null ? (
                <ChosenSkill
                  key={picked}
                  name={picked}
                  draft={drafts.take(picked, () => null)}
                  drafts={{ hold: drafts.hold(picked), drop: () => drafts.drop(picked) }}
                  saved={saved?.name === picked ? saved.commit : undefined}
                  builtIn={skills.data.find((skill) => skill.name === picked)?.built_in ?? false}
                  onNamed={(name, commit) => {
                    setSaved({ name, commit });
                    select(name);
                  }}
                  onDeleted={() => select(null)}
                />
              ) : (
                <Empty
                  title="Nothing chosen"
                  body="Pick a skill on the left to read it, or write one to give an agent something to start from."
                />
              )
            }
          />
        </div>
      )}
    </>
  );
}

/** Reads the files of the skill `name`, and opens the editor on them, or on what was typed for it. */
function ChosenSkill({
  name,
  builtIn,
  draft,
  drafts,
  saved,
  onNamed,
  onDeleted,
}: {
  name: string;
  builtIn: boolean;
  draft: SkillDraft | null;
  drafts: { hold: (draft: SkillDraft) => void; drop: () => void };
  saved: string | null | undefined;
  onNamed: (name: string, commit: string | null) => void;
  onDeleted: () => void;
}) {
  const detail = useQuery({
    queryKey: keys.skill(name),
    queryFn: () => readSkill(name),
    retry: false,
  });
  if (detail.isPending) return <p className="m-0 text-base text-secondary">Reading the skill</p>;
  if (detail.isError) {
    return <Notice tone="error">{`Could not read ${name}. ${detail.error.message}`}</Notice>;
  }
  return (
    <SkillEditor
      name={name}
      naming={false}
      builtIn={builtIn}
      source={detail.data.source}
      detail={detail.data}
      draft={draft}
      drafts={drafts}
      saved={saved}
      onNamed={onNamed}
      onDeleted={onDeleted}
    />
  );
}
