/**
 * The Get started page (the desktop app's `screens/start`): what a server still needs before the first card can run
 * (an upstream with models, Pi-Bolt, a harness configuration repository, the Coder's model, a repository), read from
 * `GET /v1/start`, with a button to the page that fixes each step, or for the repository the "Set up" that makes it.
 * It is read again when the page opens, by its Refresh button, and when a repository comes or goes. The sidebar lists
 * the page only while a step is missing.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router";
import { start } from "../../api/calls/server";
import { restartServer, setUpHarness } from "../../api/calls/settings";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { Loader } from "../../ui/components/Loader";
import { Box } from "../../ui/page/Box";
import { Notice } from "../../ui/page/Notice";
import { Header } from "../shell/Page";
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

  return (
    <>
      <div className="flex items-start justify-between gap-base">
        <Header breadcrumb="Setup" title="Get started" />
        <Button
          size="sm"
          variant="outline"
          disabled={answer.isFetching}
          onClick={() => void answer.refetch()}
        >
          Refresh
        </Button>
      </div>
      <p className="m-0 mb-[12px] text-[12.5px] text-normal">{context(answer.data)}</p>
      <div className="flex flex-col gap-[12px]">
        {answer.isError && <Notice tone="error">{answer.error.message}</Notice>}
        {setUp.isSuccess && (
          <p role="status" className="m-0 text-[12.5px] text-normal">
            Set up. The server restarts and the page reconnects by itself.
          </p>
        )}
        {answer.isPending && <Loader message="Reading the steps…" className="py-double" />}
        {answer.data && (
          <>
            {answer.data.warnings.map((warning) => (
              <p
                key={warning}
                className="m-0 rounded-md border border-[var(--amber-border)] bg-amber-fill p-base text-[13px] text-amber"
              >
                {warning}
              </p>
            ))}
            <Box title="Before the first card">
              <ol className="m-0 flex list-none flex-col p-0">
                {answer.data.steps.map((step) => {
                  const fixing = step.done ? null : fix(step.key);
                  const harness = step.key === "harness" && !step.done;
                  return (
                    <li
                      key={step.key}
                      data-testid="start-step"
                      className="flex flex-wrap items-center gap-[16px] border-b py-[12px] first:pt-0 last:border-b-0 last:pb-0"
                    >
                      <span className="w-[64px] shrink-0">
                        <Tag tone={step.done ? "teal" : "amber"}>
                          {step.done ? "Done" : "To do"}
                        </Tag>
                      </span>
                      <div className="min-w-0 flex-1">
                        <div className="text-[13px] font-semibold text-high">{step.title}</div>
                        <div className="text-[12.5px] text-normal">{step.detail}</div>
                        {harness && setUp.isError && (
                          <div className="mt-half">
                            <Notice tone="error">
                              {isApiError(setUp.error)
                                ? setUp.error.message
                                : "The repository could not be set up."}
                            </Notice>
                          </div>
                        )}
                      </div>
                      {harness && (
                        <Button
                          size="sm"
                          disabled={setUp.isPending || setUp.isSuccess}
                          onClick={() => setUp.mutate()}
                        >
                          {setUp.isPending
                            ? "Setting up…"
                            : setUp.isSuccess
                              ? "Restarting…"
                              : "Set up"}
                        </Button>
                      )}
                      {fixing && (
                        <Button asChild size="sm" variant="outline">
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
