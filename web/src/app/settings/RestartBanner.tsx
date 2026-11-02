/**
 * The banner that says a restart is needed: keys of `moebius.toml` that the running server uses only after it
 * restarts (`restart_keys` of `GET /v1/settings`), with a Restart button. While the server restarts it says so; the
 * event stream reconnects by itself and the next answer of the settings, which has nothing left to restart, takes the
 * banner away. The Settings and Models pages both show it.
 *
 * A restart is something the server needs from a person, so the banner is the one attention-coloured card of the
 * page, with the word "Restart" beside the hue.
 */
import { WarningCircleIcon } from "@phosphor-icons/react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { restartServer, settings } from "../../api/calls/settings";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Notice } from "../../ui/page/Notice";

export function RestartBanner() {
  const data = useQuery({ queryKey: keys.settings, queryFn: () => settings() });
  const [restarting, setRestarting] = useState(false);
  const restart = useMutation({
    mutationFn: restartServer,
    onMutate: () => setRestarting(true),
    onError: () => setRestarting(false),
  });
  const needed = data.data?.restart_needed === true;
  // The answer after the server is back says nothing is left to restart.
  useEffect(() => {
    if (!needed) setRestarting(false);
  }, [needed]);

  if (restarting) {
    return (
      <div
        role="status"
        className="flex items-start gap-4 rounded-card bg-attention-fill p-4 text-attention-text"
      >
        <WarningCircleIcon className="mt-1 size-4 shrink-0" aria-hidden="true" />
        <p className="m-0 min-w-0 break-words text-base">
          Restarting the server. The page reconnects by itself.
        </p>
      </div>
    );
  }
  if (!needed || !data.data) {
    return restart.isError ? (
      <Notice tone="error">
        {sayError(restart.error, "Could not restart the server. Try again.")}
      </Notice>
    ) : null;
  }
  return (
    <div className="flex flex-col gap-4">
      <section className="flex flex-col gap-4 rounded-card bg-attention-fill p-4 sm:flex-row sm:items-start">
        <div className="flex min-w-0 flex-1 items-start gap-4 text-attention-text">
          <WarningCircleIcon className="mt-1 size-4 shrink-0" aria-hidden="true" />
          <p className="m-0 min-w-0 break-words text-base">
            {`Restart the server to use: ${data.data.restart_keys.join(", ")}`}
          </p>
        </div>
        <Button size="sm" disabled={restart.isPending} onClick={() => restart.mutate()}>
          Restart
        </Button>
      </section>
      {restart.isError && (
        <Notice tone="error">
          {sayError(restart.error, "Could not restart the server. Try again.")}
        </Notice>
      )}
    </div>
  );
}
