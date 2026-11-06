/**
 * The "Add a repository" page (the former desktop app's `screens/add_repository`): the name of a repository, where to clone it
 * from, and — for one on a forge — the repository there and the authors whose issues need no approval. The server
 * clones it and opens its board, so the form is one card with one action. What the server answers when it refuses stays
 * beside the field it belongs to, in words a person can act on: the command a run printed is detail to open, not the
 * headline. Everything typed stays while the request is going and after it fails.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate } from "react-router";
import { addRepository } from "../../api/calls/projects";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Forge } from "../../api/types/board";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Chip } from "../../ui/page/Chip";
import { Field } from "../../ui/page/Field";
import { PageHeader } from "../../ui/page/PageHeader";
import { boardPath } from "../pages";
import {
  addFailure,
  EMPTY_FORM,
  type Failure,
  type RepositoryForm,
  repositoryRequest,
} from "./model";

const FORGES: ReadonlyArray<{ forge: Forge; label: string }> = [
  { forge: "none", label: "None" },
  { forge: "github", label: "GitHub" },
  { forge: "gitlab", label: "GitLab" },
  { forge: "forgejo", label: "Forgejo" },
];

/** Where the issues of a repository on each forge come from, said once for the hint of the field. */
const FORGE_NAMES: Record<Forge, string> = {
  none: "",
  github: "GitHub logins",
  gitlab: "GitLab usernames",
  forgejo: "Forgejo logins",
};

export function AddRepositoryPage() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [form, setForm] = useState<RepositoryForm>(EMPTY_FORM);
  const set = (change: Partial<RepositoryForm>) =>
    setForm((current) => ({ ...current, ...change }));
  const request = repositoryRequest(form);
  const add = useMutation({
    mutationFn: addRepository,
    onSuccess: (repository) => {
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      void queryClient.invalidateQueries({ queryKey: keys.start });
      navigate(boardPath(repository.id));
    },
  });
  const ready = request !== null && !add.isPending;
  const failure = add.isError ? addFailure(messageOf(add.error)) : null;
  // Each answer sits under the field it is about, or under the whole form when it is about neither.
  const nameFailure = failure?.field === "name" ? failure : null;
  const urlFailure = failure?.field === "url" ? failure : null;
  const formFailure = failure?.field === null ? failure : null;

  return (
    <>
      <PageHeader
        title="Add a repository"
        context="Moebius clones it on the server and opens its board."
      />
      <Box className="w-full max-w-md">
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (request && ready) add.mutate(request);
          }}
        >
          <div className="flex flex-col gap-2">
            <Field label="Name" hint="What the board calls it, such as radiance.">
              <Input
                value={form.name}
                placeholder="radiance"
                autoComplete="off"
                autoFocus
                onChange={(e) => set({ name: e.target.value })}
              />
            </Field>
            {nameFailure !== null && <FailureNote failure={nameFailure} />}
          </div>
          <div className="flex flex-col gap-2">
            <Field
              label="URL or path"
              hint="The address to clone, or a folder on the server. A private repository needs a token on the server."
            >
              <Input
                value={form.url}
                placeholder="https://github.com/acme/radiance.git"
                autoComplete="off"
                spellCheck={false}
                mono
                onChange={(e) => set({ url: e.target.value })}
              />
            </Field>
            {urlFailure !== null && <FailureNote failure={urlFailure} />}
          </div>
          <fieldset className="m-0 border-0 p-0">
            <legend className="p-0 text-sm font-semibold text-secondary">Forge</legend>
            <div className="mt-2 flex flex-wrap gap-2">
              {FORGES.map(({ forge, label }) => (
                <Chip key={forge} selected={form.forge === forge} onClick={() => set({ forge })}>
                  {label}
                </Chip>
              ))}
            </div>
            <p className="m-0 mt-2 text-sm text-secondary">
              Where the issues come from. Without one, cards are made here.
            </p>
          </fieldset>
          {form.forge === "github" && (
            <Field label="GitHub repository" hint="Owner and name, as on github.com.">
              <Input
                value={form.github}
                placeholder="acme/radiance"
                autoComplete="off"
                onChange={(e) => set({ github: e.target.value })}
              />
            </Field>
          )}
          {form.forge === "gitlab" && (
            <Field
              label="GitLab project"
              hint="Project path, groups first. The server uses the GitLab token in its settings."
            >
              <Input
                value={form.project}
                placeholder="acme/platform/radiance"
                autoComplete="off"
                onChange={(e) => set({ project: e.target.value })}
              />
            </Field>
          )}
          {form.forge === "forgejo" && (
            <Field
              label="Forgejo repository"
              hint="Owner and name, as on Codeberg. The server uses the Forgejo token in its settings."
            >
              <Input
                value={form.project}
                placeholder="acme/radiance"
                autoComplete="off"
                onChange={(e) => set({ project: e.target.value })}
              />
            </Field>
          )}
          {form.forge !== "none" && (
            <Field
              label="Trusted authors"
              hint={`${FORGE_NAMES[form.forge]}, comma separated. Their issues need no approval.`}
            >
              <Input
                value={form.trusted}
                placeholder="leon, mona"
                autoComplete="off"
                onChange={(e) => set({ trusted: e.target.value })}
              />
            </Field>
          )}
          {formFailure !== null && <FailureNote failure={formFailure} />}
          <Button type="submit" disabled={!ready}>
            {add.isPending ? "Adding" : "Add"}
          </Button>
        </form>
      </Box>
    </>
  );
}

/**
 * The sentence the server gave. A request that never reached it has no sentence of its own, and the one the client
 * makes for that is said here rather than left for the classifier to translate.
 */
function messageOf(error: unknown): string {
  if (isApiError(error) && !error.unreachable) return error.message;
  return "The server could not be reached. Try again.";
}

/**
 * A refusal, beside the field it belongs to: what happened, what to do next, and the exact sentence the server gave
 * under "What the server said" for someone who wants the command it ran.
 */
function FailureNote({ failure }: { failure: Failure }) {
  return (
    <div
      role="alert"
      className="flex flex-col gap-2 rounded-control bg-error-fill p-3 text-error-text"
    >
      <p className="m-0 text-sm font-semibold">{failure.headline}</p>
      {failure.next !== "" && <p className="m-0 text-sm">{failure.next}</p>}
      {failure.detail !== null && (
        <details className="text-sm">
          <summary className="flex min-h-10 cursor-pointer items-center">
            What the server said
          </summary>
          <pre className="m-0 mt-2 break-all rounded-small bg-sunken p-2 font-mono text-xs whitespace-pre-wrap text-secondary">
            {failure.detail}
          </pre>
        </details>
      )}
    </div>
  );
}
