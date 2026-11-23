/**
 * The Get started page (the former desktop app's `screens/start`): what a server still needs before the first card can run
 * (an upstream with models, Pi-Bolt, a harness configuration repository, the Coder's model, a repository), read from
 * `GET /v1/start`. Each step is a row with its status icon, its name and what it still needs, and the button that fixes it at the right;
 * the one to take next has the primary button and the steps already taken have none. Finishing a step moves it as soon as the page is read again: by its
 * Refresh button, when the page opens, and when a repository comes or goes. The sidebar lists the page only while a
 * step is missing.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { Link } from "react-router";
import { start } from "../../api/calls/server";
import { restartServer, setUpHarness } from "../../api/calls/settings";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { cn } from "../../ui/lib/cn";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { StatusIcon } from "../../ui/page/StatusIcon";
import { useToast } from "../../ui/page/Toast";
import { fix } from "./model";

export function StartPage() {
  const queryClient = useQueryClient();
  const toast = useToast();
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
    onSuccess: () =>
      toast({
        kind: "confirmation",
        text: "Set up. The server restarts and the page reconnects by itself.",
      }),
    onError: (error) =>
      toast({ kind: "error", text: sayError(error, "The repository could not be set up.") }),
    onSettled: () => void queryClient.invalidateQueries({ queryKey: keys.start }),
  });
  const steps = answer.data?.steps ?? [];
  const pending = answer.data?.pending ?? [];
  const next = steps.find((step) => !step.done);
  const warnings = answer.data?.warnings ?? [];
  // A warning is an error the server reports with the steps; each is told once, by a toast.
  const told = warnings.join("\n");
  // biome-ignore lint/correctness/useExhaustiveDependencies: `toast` is stable and `told` stands for the warnings
  useEffect(() => {
    for (const warning of told === "" ? [] : told.split("\n"))
      toast({ kind: "error", text: warning });
  }, [told]);

  return (
    <>
      <PageHeader
        title="Get started"
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
      {answer.isError && (
        <ReadFailed
          error={answer.error}
          fallback="The steps could not be read. Try again."
          busy={answer.isFetching}
          onRetry={() => void answer.refetch()}
        />
      )}
      {answer.isPending && <SkeletonList rows={5} message="Reading the steps" />}
      {answer.data && (
        <ol className="m-0 flex list-none flex-col p-0">
          {steps.map((step) => {
            const fixing = step.done ? null : fix(step.key);
            const harness = step.key === "harness" && !step.done;
            // The step to take next gets the primary button; the ones already taken stay quiet.
            const lead = step.key === next?.key;
            return (
              <li
                key={step.key}
                data-testid="start-step"
                className="flex min-h-9 items-center gap-2.5 border-b border-hairline px-4 py-2 text-base"
              >
                <StatusIcon state={step.done ? "done" : "notStarted"} />
                <span className="flex min-w-0 flex-1 flex-col">
                  <span
                    className={cn(
                      "break-words font-medium",
                      step.done ? "text-muted" : "text-text",
                    )}
                  >
                    {step.title}
                  </span>
                  {!step.done && (
                    <span className="break-words text-sm text-muted">{step.detail}</span>
                  )}
                </span>
                {harness && (
                  <Button
                    size="sm"
                    variant={lead ? "primary" : "secondary"}
                    disabled={setUp.isPending || setUp.isSuccess}
                    onClick={() => setUp.mutate()}
                  >
                    {setUp.isPending ? "Setting up" : setUp.isSuccess ? "Restarting" : "Set up"}
                  </Button>
                )}
                {fixing && (
                  <Button asChild size="sm" variant={lead ? "primary" : "secondary"}>
                    <Link to={fixing.to}>{fixing.label}</Link>
                  </Button>
                )}
              </li>
            );
          })}
        </ol>
      )}
      {pending.length > 0 && (
        <section aria-label="Waiting" className="mt-6 flex flex-col">
          <h3 className="m-0 px-4 pb-2 text-base font-semibold text-text">Waiting</h3>
          <ul className="m-0 flex list-none flex-col p-0">
            {pending.map((step) => (
              <li
                key={`${step.key}:${step.title}`}
                data-testid="start-pending"
                className="flex min-h-9 items-center gap-2.5 border-b border-hairline px-4 py-2 text-base"
              >
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="break-words font-medium text-text">{step.title}</span>
                  <span className="break-words text-sm text-muted">{step.detail}</span>
                </span>
                {step.key === "wiki" && (
                  <Button asChild size="sm" variant="secondary">
                    <Link to="/wiki">Open Wiki</Link>
                  </Button>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}
    </>
  );
}
