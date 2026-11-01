/**
 * The "Add a repository" page (the desktop app's `screens/add_repository`): a name, where to clone from, the forge
 * (none, GitHub, GitLab or Forgejo) with the repository's name there, and the trusted authors. Add sends
 * `POST /v1/repositories`; the new board opens when the server took it, and what it refused is shown under the form.
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
import { Notice } from "../../ui/page/Notice";
import { boardPath } from "../pages";
import { Header } from "../shell/Page";
import { EMPTY_FORM, type RepositoryForm, repositoryRequest } from "./model";

const FORGES: ReadonlyArray<{ forge: Forge; label: string }> = [
  { forge: "none", label: "None" },
  { forge: "github", label: "GitHub" },
  { forge: "gitlab", label: "GitLab" },
  { forge: "forgejo", label: "Forgejo" },
];

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

  return (
    <>
      <Header breadcrumb="Projects" title="Add a repository" />
      <p className="m-0 mb-[12px] text-[12.5px] text-normal">
        The server clones it and opens its board.
      </p>
      <Box title="Repository">
        <form
          className="flex max-w-[640px] flex-col gap-[12px]"
          onSubmit={(e) => {
            e.preventDefault();
            if (request && ready) add.mutate(request);
          }}
        >
          <Field label="Name">
            <Input
              value={form.name}
              placeholder="radiance"
              onChange={(e) => set({ name: e.target.value })}
              autoFocus
            />
          </Field>
          <Field
            label="URL or path"
            hint="Anything git clone takes, or a path on the server. A new or empty path becomes a new repository."
          >
            <Input
              value={form.url}
              placeholder="https://github.com/acme/radiance.git"
              onChange={(e) => set({ url: e.target.value })}
            />
          </Field>
          <div className="flex flex-col gap-half">
            <span className="text-xs font-semibold text-normal">Forge</span>
            <div className="flex flex-wrap gap-base">
              {FORGES.map(({ forge, label }) => (
                <Chip key={forge} selected={form.forge === forge} onClick={() => set({ forge })}>
                  {label}
                </Chip>
              ))}
            </div>
          </div>
          {form.forge === "github" && (
            <Field label="GitHub repository">
              <Input
                value={form.github}
                placeholder="acme/radiance"
                onChange={(e) => set({ github: e.target.value })}
              />
            </Field>
          )}
          {form.forge === "gitlab" && (
            <Field
              label="GitLab project"
              hint="Project path, groups first. The server uses the token of [gitlab] in its settings."
            >
              <Input
                value={form.project}
                placeholder="acme/platform/radiance"
                onChange={(e) => set({ project: e.target.value })}
              />
            </Field>
          )}
          {form.forge === "forgejo" && (
            <Field
              label="Forgejo repository"
              hint="Owner and name, as on Codeberg. The server uses the token of [forgejo] in its settings."
            >
              <Input
                value={form.project}
                placeholder="acme/radiance"
                onChange={(e) => set({ project: e.target.value })}
              />
            </Field>
          )}
          {form.forge !== "none" && (
            <Field
              label="Trusted authors"
              hint={`${form.forge === "github" ? "GitHub logins" : form.forge === "gitlab" ? "GitLab usernames" : "Forgejo logins"}, comma separated. Their issues need no approval.`}
            >
              <Input
                value={form.trusted}
                placeholder="leon, mona"
                onChange={(e) => set({ trusted: e.target.value })}
              />
            </Field>
          )}
          <div>
            <Button type="submit" disabled={!ready}>
              {add.isPending ? "Adding…" : "Add"}
            </Button>
          </div>
          {add.isError && (
            <Notice tone="error">
              {isApiError(add.error) ? add.error.message : "The repository could not be added."}
            </Notice>
          )}
        </form>
      </Box>
    </>
  );
}
