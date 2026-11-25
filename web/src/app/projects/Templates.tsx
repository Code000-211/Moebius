/**
 * The "Templates" box of a repository on a forge (ADR 0034): the pull request template and the comment templates in
 * effect, where each comes from (the default Moebius ships or the project's own) and where the repository keeps its copy,
 * and the button that opens the pull request which keeps those copies current. `.github/` is a locked path, so that pull
 * request waits for the owner's click in the Inbox.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { readTemplates, syncTemplates } from "../../api/calls/factory";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Box } from "../../ui/page/Box";
import { Notice } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";

export function Templates({ repository }: { repository: number }) {
  const queryClient = useQueryClient();
  const templates = useQuery({
    queryKey: keys.repositoryTemplates(repository),
    queryFn: () => readTemplates(repository),
  });
  const sync = useMutation({
    mutationFn: () => syncTemplates(repository),
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: keys.repositoryTemplates(repository) }),
  });
  return (
    <Box title="Templates" testId="templates" strong>
      {templates.data ? (
        <div className="flex max-w-xl flex-col gap-4">
          <p className="m-0 text-base text-secondary">
            Every pull request and comment is read by its template, so a card and the Reviewer read
            what people did. A project changes a template in the harness configuration repository.
          </p>
          <div className="text-sm text-muted" data-testid="template-sections">
            {`Pull request sections: ${templates.data.sections
              .map((s) => (s.required ? s.heading : `${s.heading} (optional)`))
              .join(", ")}`}
          </div>
          {templates.data.problem && <Notice tone="error">{templates.data.problem}</Notice>}
          <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
            {templates.data.templates.map((t) => (
              <li
                key={t.name}
                className="flex min-w-0 flex-wrap items-baseline justify-between gap-x-3 text-sm"
              >
                <span className="text-text">
                  {t.name === "pull_request" ? "Pull request" : t.first_line || t.name}
                </span>
                <span className="text-muted">{`${t.source === "project" ? "the project's" : "default"}, kept at ${t.path}`}</span>
              </li>
            ))}
          </ul>
          <div className="flex flex-wrap items-center gap-3">
            <Button size="sm" disabled={sync.isPending} onClick={() => sync.mutate()}>
              {sync.isPending ? "Opening" : "Update the repository's copies"}
            </Button>
            <span className="text-sm text-muted">
              Opens a pull request for .github/. It waits for your click.
            </span>
          </div>
          {sync.data && (
            <Notice>
              {sync.data.change
                ? `Pull request ${sync.data.change.pull_request ? `#${sync.data.change.pull_request}` : "opened"}: it is in your Inbox.`
                : "The repository's copies are current."}
            </Notice>
          )}
          {sync.isError && (
            <Notice tone="error">
              {isApiError(sync.error) ? sync.error.message : "The copies could not be updated."}
            </Notice>
          )}
        </div>
      ) : templates.isError ? (
        <Notice tone="error">The templates could not be read. Try again.</Notice>
      ) : (
        <SkeletonLines lines={3} />
      )}
    </Box>
  );
}
