/**
 * The banner that says a restart is needed: keys of `moebius.toml` that the running server uses only after it
 * restarts (`restart_keys` of `GET /v1/settings`), with a Restart button. While the server restarts it says so; the
 * event stream reconnects by itself and the next answer of the settings, which has nothing left to restart, takes the
 * banner away. The Settings and Models pages both show it.
 */
import { useMutation, useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { restartServer, settings } from "../../api/calls/settings";
import { isApiError } from "../../api/http";
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
        className="rounded-md border border-[var(--amber-border)] bg-amber-fill p-base text-[13px] text-amber"
      >
        Restarting the server. The page reconnects by itself.
      </div>
    );
  }
  if (!needed || !data.data) {
    return restart.isError ? (
      <Notice tone="error">
        {isApiError(restart.error) ? restart.error.message : "The server could not be restarted."}
      </Notice>
    ) : null;
  }
  return (
    <div className="flex flex-col gap-half">
      <div className="flex flex-wrap items-center gap-base rounded-md border border-[var(--amber-border)] bg-amber-fill p-base">
        <span className="min-w-0 flex-1 text-[13px] text-amber">
          {`Restart the server to use: ${data.data.restart_keys.join(", ")}`}
        </span>
        <Button size="sm" onClick={() => restart.mutate()}>
          Restart
        </Button>
      </div>
      {restart.isError && (
        <Notice tone="error">
          {isApiError(restart.error) ? restart.error.message : "The server could not be restarted."}
        </Notice>
      )}
    </div>
  );
}
