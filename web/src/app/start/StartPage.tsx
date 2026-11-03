/**
 * The Get started page (the desktop app's `screens/start`): what a server still needs before the first card can run
 * (an upstream with models, Pi-Bolt, a harness configuration repository, the Coder's model, a repository), read from
 * `GET /v1/start`. The step to take next is the one the page is built around, with the page that fixes it beside it;
 * the steps already taken are named but quiet. Finishing a step moves it as soon as the page is read again: by its
 * Refresh button, when the page opens, and when a repository comes or goes. The sidebar lists the page only while a
 * step is missing.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router";
import { start } from "../../api/calls/server";
import { restartServer, setUpHarness } from "../../api/calls/settings";
import { sayError } from "../../api/errorText";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { cn } from "../../ui/lib/cn";
import { Box } from "../../ui/page/Box";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { Status } from "../../ui/page/Status";
import { context, fix } from "./model";

export function StartPage() {
  const queryClient = useQueryClient();
  const answer = useQuery({
    queryKey: keys.start,
    queryFn: () => start(),
    // The page reads the steps each time it opens.
    staleTime: 0,
    refetchOnMount: "always",
  });
  // The harness repository is made here, and the server restarts to use it; the page reconnects by itself.
  const setUp = useMutation({
    mutationFn: async () => {
      await setUpHarness();
      await restartServer();
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey: keys.start }),
  });
  const steps = answer.data?.steps ?? [];
  const next = steps.find((step) => !step.done);

  return (
    <>
      <PageHeader
        title="Get started"
        context={context(answer.data)}
        actions={
          <Button
            size="sm"
            variant="secondary"
            disabled={answer.isFetching}
            onClick={() => void answer.refetch()}
          >
            Refresh
          </Button>
        }
      />
      <div className="flex flex-col gap-4">
        {answer.isError && (
          <Notice tone="error">
            {sayError(answer.error, "The steps could not be read. Try again.")}
          </Notice>
        )}
        {setUp.isSuccess && (
          <Notice tone="ok">Set up. The server restarts and the page reconnects by itself.</Notice>
        )}
        {answer.isPending && <SkeletonCard lines={4} message="Reading the steps" />}
        {answer.data && (
          <>
            {answer.data.warnings.map((warning) => (
              <p
                key={warning}
                role="status"
                className="m-0 break-words rounded-control bg-attention-fill px-3 py-3 text-base text-attention-text"
              >
                {warning}
              </p>
            ))}
            <Box title="Before the first card" aside={next ? `Next: ${next.title}` : undefined}>
              <ol className="m-0 flex list-none flex-col gap-2 p-0">
                {steps.map((step) => {
                  const fixing = step.done ? null : fix(step.key);
                  const harness = step.key === "harness" && !step.done;
                  // The step to take next is the page's focal point; the ones already taken stay quiet.
                  const lead = step.key === next?.key;
                  return (
                    <li
                      key={step.key}
                      data-testid="start-step"
                      className={cn(
                        "flex min-h-10 flex-wrap items-center gap-4 rounded-control p-3",
                        lead && "bg-sunken",
                      )}
                    >
                      <Status tone={step.done ? "complete" : "attention"} className="shrink-0">
                        {step.done ? "Done" : "To do"}
                      </Status>
                      <span className="flex min-w-0 flex-1 flex-col gap-1">
                        <span
                          className={cn(
                            "break-words font-semibold text-text",
                            lead ? "text-lg" : "text-base",
                          )}
                        >
                          {step.title}
                        </span>
                        <span className="break-words text-base text-secondary">{step.detail}</span>
                        {harness && setUp.isError && (
                          <Notice tone="error">
                            {isApiError(setUp.error)
                              ? setUp.error.message
                              : "The repository could not be set up."}
                          </Notice>
                        )}
                      </span>
                      {harness && (
                        <Button
                          size="sm"
                          variant={lead ? "primary" : "secondary"}
                          disabled={setUp.isPending || setUp.isSuccess}
                          onClick={() => setUp.mutate()}
                        >
                          {setUp.isPending
                            ? "Setting up"
                            : setUp.isSuccess
                              ? "Restarting"
                              : "Set up"}
                        </Button>
                      )}
                      {fixing && (
                        <Button asChild variant={lead ? "primary" : "secondary"}>
                          <Link to={fixing.to}>{fixing.label}</Link>
                        </Button>
                      )}
                    </li>
                  );
                })}
              </ol>
            </Box>
          </>
        )}
      </div>
    </>
  );
}
