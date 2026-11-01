/**
 * Setting up a harness configuration repository from the app (the desktop app's `harness_setup`). A server without one
 * keeps its agents and rules in `moebius.toml` and has no skills or MCP servers; the Agents, Skills and MCP servers
 * pages then show this box with "Set up". Pressed, it sends `POST /v1/harness/setup` and asks the server to restart;
 * the event stream reconnects by itself and every page reads again, so once the server is back the box is gone.
 */
import { useMutation } from "@tanstack/react-query";
import { setUpHarness } from "../../api/calls/harness";
import { restartServer } from "../../api/calls/models";
import { Button } from "../../ui/components/Button";
import { Box } from "../../ui/page/Box";
import { Notice } from "../../ui/page/Notice";

/** What the box says, and why the buttons that need a repository are quiet. */
export const NEEDED =
  "Skills, MCP servers and imports need a harness configuration repository. Set it up here; your agents and rules move into it.";

/** The reason a button that needs the repository is disabled, for its title and for the line under it. */
export const WHY_MUTED = "Needs a harness configuration repository: set it up below.";

export function HarnessSetup() {
  const setUp = useMutation({
    mutationFn: async () => {
      await setUpHarness();
      await restartServer();
    },
  });
  const label = setUp.isPending ? "Setting up" : setUp.isSuccess ? "Restarting" : "Set up";
  return (
    <Box
      testId="harness-setup"
      className="flex flex-wrap items-center justify-between gap-4"
      padded
    >
      <div className="flex min-w-0 flex-1 flex-col gap-2">
        <p className="m-0 max-w-prose text-base text-text">{NEEDED}</p>
        {setUp.isSuccess && (
          <p className="m-0 text-sm text-secondary">
            Set up. The server restarts and the app reconnects by itself.
          </p>
        )}
        {setUp.isError && <Notice tone="error">{setUp.error.message}</Notice>}
      </div>
      <Button
        size="sm"
        disabled={setUp.isPending || setUp.isSuccess}
        onClick={() => setUp.mutate()}
      >
        {label}
      </Button>
    </Box>
  );
}
