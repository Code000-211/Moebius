/**
 * The page of one repository (`/projects/:id`; the desktop app's `screens/project`): where it is and what it trusts,
 * with the logins of a repository on a forge to edit, and its files on the default branch, which can be read, edited
 * and saved as a commit. The board of the repository is its own page.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useParams } from "react-router";
import { snapshot as readSnapshot } from "../../api/calls/board";
import { editTrusted } from "../../api/calls/projects";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Repository } from "../../api/types/board";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Loader } from "../../ui/components/Loader";
import { Box } from "../../ui/page/Box";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { boardPath } from "../pages";
import { Header } from "../shell/Page";
import { FilesPanel } from "./FilesPanel";
import { forgeTag, logins, onForge } from "./model";

export function ProjectPage() {
  const { id } = useParams();
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  if (data.isPending) return <Loader message="Reading the project…" className="py-double" />;
  const board = data.data?.boards.find((each) => String(each.repository.id) === id);
  if (!board) {
    return (
      <>
        <Header breadcrumb="Projects" title="Project" />
        <p className="text-sm text-normal">
          There is no such repository. Pick one from the{" "}
          <Link to="/projects" className="text-accent">
            Projects
          </Link>
          .
        </p>
      </>
    );
  }
  const { repository } = board;
  return (
    <>
      <div className="flex items-start justify-between gap-base">
        <Header breadcrumb="Projects" title={repository.name} />
        <Button asChild size="sm" variant="outline">
          <Link to={boardPath(repository.id)}>Open board</Link>
        </Button>
      </div>
      <div className="flex flex-col gap-[12px]">
        <Box title="Settings">
          <dl className="m-0 mb-[12px] flex flex-wrap gap-base">
            <Fact label="Forge" value={forgeTag(repository)} />
            <Fact label="Default branch" value={repository.default_branch} mono />
            <Fact label="Clone on the server" value={repository.path} mono />
          </dl>
          {onForge(repository) ? (
            <Trusted key={repository.trusted.join(",")} repository={repository} />
          ) : (
            <p className="m-0 text-[13px] text-normal">
              This repository is on no forge, so it has no trusted authors: its issues come from
              here.
            </p>
          )}
        </Box>
        <FilesPanel repository={repository.id} />
      </div>
    </>
  );
}

function Fact({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="min-w-[160px] flex-1 rounded-md border bg-lane p-[10px]">
      <dt className="font-mono text-[11px] text-low">{label}</dt>
      <dd className={`m-0 break-all text-[13px] text-high ${mono ? "font-mono text-[12px]" : ""}`}>
        {value}
      </dd>
    </div>
  );
}

/** The logins whose issues need no approval, as a line with a pencil that opens a field to change them. */
function Trusted({ repository }: { repository: Repository }) {
  const queryClient = useQueryClient();
  const [editing, setEditing] = useState<string | null>(null);
  const save = useMutation({
    mutationFn: (typed: string) => editTrusted(repository.id, logins(typed)),
    onSuccess: () => {
      setEditing(null);
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
    },
  });
  if (editing === null) {
    return (
      <div className="flex flex-wrap items-center gap-base">
        <span className="font-mono text-[12.5px] text-normal" data-testid="trusted">
          {repository.trusted.length === 0
            ? "No trusted authors"
            : `Trusts ${repository.trusted.join(", ")}`}
        </span>
        <Button
          size="xs"
          variant="outline"
          onClick={() => {
            save.reset();
            setEditing(repository.trusted.join(", "));
          }}
        >
          Edit trusted authors
        </Button>
      </div>
    );
  }
  return (
    <form
      className="flex flex-col gap-base"
      onSubmit={(e) => {
        e.preventDefault();
        if (!save.isPending) save.mutate(editing);
      }}
    >
      <Field
        label="Trusted authors"
        hint="Logins on the forge, comma separated. Their issues need no approval."
      >
        <Input
          value={editing}
          placeholder="leon, mona"
          onChange={(e) => setEditing(e.target.value)}
          autoFocus
        />
      </Field>
      <div className="flex gap-base">
        <Button type="submit" size="sm" disabled={save.isPending}>
          {save.isPending ? "Saving…" : "Save"}
        </Button>
        <Button type="button" size="sm" variant="outline" onClick={() => setEditing(null)}>
          Cancel
        </Button>
      </div>
      {save.isError && (
        <Notice tone="error">
          {isApiError(save.error) ? save.error.message : "The logins could not be saved."}
        </Notice>
      )}
    </form>
  );
}
