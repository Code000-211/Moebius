/**
 * The settings Moebius keeps for one repository, the files of `repos/<name>/` in the harness configuration
 * repository: the notes for its reviews, the lint plan of the lens, the checks of a repository without a forge, the
 * hint of likely files, and the switch of test first. Each is saved as a commit there, which the server checks first.
 * Its rules are edited on the Rules page and its column scripts on the board; this panel says which scripts it has. A
 * `.moebius/` folder that the repository still has is said to be no longer read.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { readSettings, removeSetting, saveSetting } from "../../api/calls/projects";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { SettingFile } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { Box } from "../../ui/page/Box";
import { Field, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { boardPath } from "../pages";
import { firstLine, SETTING_FILES } from "./model";

export function SettingsPanel({ repository }: { repository: number }) {
  const settings = useQuery({
    queryKey: keys.projectSettings(repository),
    queryFn: () => readSettings(repository),
  });
  if (settings.isPending)
    return (
      <Box title="Project settings">
        <SkeletonCard lines={3} message="Reading the project settings" />
      </Box>
    );
  if (settings.isError)
    return (
      <Box title="Project settings">
        <ReadFailed
          error={settings.error}
          fallback="The project settings could not be read. Try again."
          busy={settings.isFetching}
          onRetry={() => void settings.refetch()}
        />
      </Box>
    );
  const { path, editable, files, scripts, leftover } = settings.data;
  const file = (name: string): SettingFile =>
    files.find((each) => each.file === name) ?? { file: name, exists: false, text: "" };
  return (
    <Box title="Project settings" aside={path}>
      <div className="flex flex-col gap-6">
        <p className="m-0 max-w-prose text-base text-secondary">
          These files are kept in the harness configuration repository, not in the repository
          itself, and every save is a commit there.{" "}
          <Link className="underline" to="/rules">
            Rules
          </Link>{" "}
          are edited on the Rules page and the column scripts on the{" "}
          <Link className="underline" to={boardPath(repository)}>
            board
          </Link>
          {scripts.length > 0 ? ` (this project has ${scripts.join(", ")})` : ""}.
        </p>
        {leftover.length > 0 && (
          <Notice tone="error">
            <span className="font-semibold">This repository still has a .moebius folder. </span>
            Nothing in it is read any more ({leftover.join(", ")}). Move the files into the settings
            above.
          </Notice>
        )}
        {!editable && (
          <Notice tone="error">
            <span className="font-semibold">
              This server has no harness configuration repository.{" "}
            </span>
            The server needs a [harness] section in moebius.toml first: until then no project has
            settings, and a .moebius folder is not read either.
          </Notice>
        )}
        <TestFirst repository={repository} file={file("test-first")} editable={editable} />
        {SETTING_FILES.map((each) => (
          <SettingEditor
            key={each.file}
            repository={repository}
            file={file(each.file)}
            label={each.label}
            hint={each.hint}
            editable={editable}
          />
        ))}
      </div>
    </Box>
  );
}

/** What a save or a removal does to the cache: the settings of every repository are read again. */
function useChange(repository: number, change: () => Promise<SettingFile>) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: change,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: keys.projectSettings(repository) });
    },
  });
}

/** The words for a refusal of the server, which says first that nothing was committed. */
function Refused({ what, error }: { what: string; error: unknown }) {
  return (
    <Notice tone="error">
      <span className="font-semibold">{what} Nothing was committed. </span>
      {isApiError(error) ? error.message : "The change could not be saved."}
    </Notice>
  );
}

/** The switch of test first: its file exists or not, and its content does not matter. */
function TestFirst({
  repository,
  file,
  editable,
}: {
  repository: number;
  file: SettingFile;
  editable: boolean;
}) {
  const change = useChange(repository, () =>
    file.exists
      ? removeSetting(repository, "test-first")
      : saveSetting(repository, "test-first", ""),
  );
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-4">
        <p className="m-0 min-w-0 flex-1 text-base text-text" data-testid="test-first">
          {file.exists ? "Works test first" : "Does not work test first"}
        </p>
        <Button
          size="sm"
          variant="secondary"
          disabled={!editable || change.isPending}
          onClick={() => change.mutate()}
        >
          {file.exists ? "Turn test first off" : "Turn test first on"}
        </Button>
      </div>
      <p className="m-0 max-w-prose text-sm text-secondary">
        The Coder writes a test that reproduces the issue before it changes any code, and a change
        is let on only when that test fails without it and passes with it.
      </p>
      {change.isError && <Refused what="Couldn't change the switch." error={change.error} />}
    </div>
  );
}

/** One file of the settings: what it holds in a line, with the way to edit it as text, to save it or to remove it. */
function SettingEditor({
  repository,
  file,
  label,
  hint,
  editable,
}: {
  repository: number;
  file: SettingFile;
  label: string;
  hint: string;
  editable: boolean;
}) {
  const [editing, setEditing] = useState<string | null>(null);
  const save = useChange(repository, async () => {
    const saved = await saveSetting(repository, file.file, editing ?? "");
    setEditing(null);
    return saved;
  });
  const remove = useChange(repository, () => removeSetting(repository, file.file));
  if (editing === null) {
    return (
      <div className="flex flex-col gap-2">
        <div className="flex flex-wrap items-center gap-4">
          <div className="min-w-0 flex-1">
            <p className="m-0 text-base font-semibold text-text">{label}</p>
            <p
              className="m-0 break-words text-sm text-secondary"
              data-testid={`setting-${file.file}`}
            >
              {file.exists ? firstLine(file.text) : "Not set."} {hint}
            </p>
          </div>
          <Button
            size="sm"
            variant="secondary"
            aria-label={`Edit ${label.toLowerCase()}`}
            disabled={!editable}
            onClick={() => {
              save.reset();
              setEditing(file.text);
            }}
          >
            Edit
          </Button>
        </div>
        {remove.isError && <Refused what={`Couldn't remove ${file.file}.`} error={remove.error} />}
      </div>
    );
  }
  return (
    <form
      className="flex flex-col gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (!save.isPending) save.mutate();
      }}
    >
      <Field label={label} hint={`${hint} The file is ${file.file}.`}>
        <TextArea mono rows={6} value={editing} onChange={(e) => setEditing(e.target.value)} />
      </Field>
      <div className="flex flex-wrap items-center gap-2">
        <Button type="submit" size="sm" disabled={save.isPending}>
          {save.isPending ? "Saving" : "Save"}
        </Button>
        <Button type="button" size="sm" variant="secondary" onClick={() => setEditing(null)}>
          Cancel
        </Button>
        {file.exists && (
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={remove.isPending}
            onClick={() => {
              remove.mutate(undefined, { onSuccess: () => setEditing(null) });
            }}
          >
            Remove the file
          </Button>
        )}
      </div>
      {save.isError && <Refused what={`Couldn't save ${file.file}.`} error={save.error} />}
    </form>
  );
}
