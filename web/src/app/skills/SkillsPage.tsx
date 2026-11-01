/**
 * The Skills page: the skills an agent can name, and the editor of the one chosen (`SkillEditor`): its description,
 * the list of its files, and the text of `SKILL.md` or of the file open. An edit of `SKILL.md` is
 * `PUT /v1/skills/{name}`, which the server makes a commit in the harness configuration repository. The skills Moebius
 * brings are shown and cannot be changed. A server without a harness configuration repository lists no skills, and
 * the page then offers to set one up. The skill chosen is in the address (`?skill=`, or `?new=1`).
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useSearchParams } from "react-router";
import { skill as readSkill, skillList } from "../../api/calls/harness";
import { keys } from "../../api/keys";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { Loader } from "../../ui/components/Loader";
import { Box, ListCard, ListEditor } from "../harness/form";
import { HarnessSetup, WHY_MUTED } from "../harness/HarnessSetup";
import { ImportPanel } from "../harness/ImportPanel";
import { Header } from "../shell/Page";
import { SkillEditor } from "./SkillEditor";

export function SkillsPage() {
  const [params, setParams] = useSearchParams();
  const [importing, setImporting] = useState(false);
  const [saved, setSaved] = useState<{ name: string; commit: string | null } | null>(null);
  const skills = useQuery({ queryKey: keys.skills, queryFn: skillList });
  const chosen = params.get("skill");
  const creating = params.get("new") === "1";
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

  // A server with a harness configuration repository always lists the built-in skills, so none means there is none.
  const editable = skills.isSuccess && skills.data.length > 0;
  const noRepository = skills.isSuccess && skills.data.length === 0;
  const muted = !editable ? WHY_MUTED : undefined;

  return (
    <>
      <div className="flex items-start justify-between gap-base">
        <Header breadcrumb="Build" title="Skills" />
        <div className="flex gap-plusfifty" title={muted}>
          <Button
            variant="outline"
            size="sm"
            disabled={!editable}
            onClick={() => setImporting(true)}
          >
            Import
          </Button>
          <Button size="sm" disabled={!editable} onClick={() => select(null, true)}>
            <PlusIcon className="size-icon-sm" weight="bold" />
            New skill
          </Button>
        </div>
      </div>
      <p className="m-0 mb-plusfifty text-sm text-normal">
        Skills an agent can be given: skills/ of the harness repository, and Moebius's own. An edit
        is a commit.
      </p>
      {skills.isPending && <Loader message="Reading the skills…" className="py-double" />}
      {skills.isError && <p className="text-sm text-error">{skills.error.message}</p>}
      {noRepository && <HarnessSetup />}
      {editable && (
        <div className="flex flex-col gap-double">
          {importing && <ImportPanel kind="skills" onClose={() => setImporting(false)} />}
          <ListEditor
            list={skills.data.map((skill) => (
              <ListCard
                key={skill.name}
                name={skill.name}
                tags={skill.built_in && <Tag>Built in</Tag>}
                line={skill.description}
                chosen={!creating && chosen === skill.name}
                onClick={() => {
                  setSaved(null);
                  select(skill.name);
                }}
              />
            ))}
          >
            {creating ? (
              <SkillEditor
                key="new"
                name=""
                naming
                builtIn={false}
                source={undefined}
                onNamed={(name, commit) => {
                  setSaved({ name, commit });
                  select(name);
                }}
                onDeleted={() => select(null)}
              />
            ) : chosen !== null ? (
              <ChosenSkill
                key={chosen}
                name={chosen}
                saved={saved?.name === chosen ? saved.commit : undefined}
                builtIn={skills.data.find((skill) => skill.name === chosen)?.built_in ?? false}
                onNamed={(name, commit) => {
                  setSaved({ name, commit });
                  select(name);
                }}
                onDeleted={() => select(null)}
              />
            ) : (
              <Box>
                <p className="m-0 text-sm text-low">Pick a skill to read or edit it.</p>
              </Box>
            )}
          </ListEditor>
        </div>
      )}
    </>
  );
}

function ChosenSkill({
  name,
  builtIn,
  saved,
  onNamed,
  onDeleted,
}: {
  name: string;
  builtIn: boolean;
  saved: string | null | undefined;
  onNamed: (name: string, commit: string | null) => void;
  onDeleted: () => void;
}) {
  const detail = useQuery({
    queryKey: keys.skill(name),
    queryFn: () => readSkill(name),
    retry: false,
  });
  if (detail.isPending) return <p className="m-0 text-sm text-normal">Reading the skill…</p>;
  if (detail.isError) {
    return (
      <Box>
        <p role="alert" className="m-0 text-xs text-error">
          {detail.error.message}
        </p>
      </Box>
    );
  }
  return (
    <SkillEditor
      name={name}
      naming={false}
      builtIn={builtIn}
      source={detail.data.source}
      detail={detail.data}
      saved={saved}
      onNamed={onNamed}
      onDeleted={onDeleted}
    />
  );
}
