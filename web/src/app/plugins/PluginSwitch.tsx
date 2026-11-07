/**
 * The switch of one plugin on the Plugins page: on or off at once (`PUT /v1/plugins/{plugin}/enabled`), the plugin
 * keeping its entry and its settings. Turning off the last launcher means runs start unwrapped, which the server refuses
 * (409) until it is confirmed, so the refusal opens a dialog that says so and asks again with `unwrap_runs`. Turning off
 * the guard lets tool calls through unchecked, so the dialog opens first, before anything is sent, and the request
 * carries `allow_unchecked`. Any other refusal is said in the server's words under the switch.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { setPluginEnabled } from "../../api/calls/models";
import { sayError } from "../../api/errorText";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../ui/components/Dialog";
import { Notice } from "../../ui/page/Notice";
import { Switch } from "../../ui/page/Switch";

/** The built-in deciders that have a switch. */
export const SWITCHED_BUILT_INS = new Set(["guard", "lens", "localize"]);

export function PluginSwitch({ name, enabled }: { name: string; enabled: boolean }) {
  const queryClient = useQueryClient();
  const [unwrapping, setUnwrapping] = useState(false);
  const [unchecking, setUnchecking] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const turn = useMutation({
    mutationFn: (change: { on: boolean; unwrapRuns: boolean; allowUnchecked?: boolean }) =>
      setPluginEnabled(name, change.on, change.unwrapRuns, change.allowUnchecked),
    onSuccess: () => {
      setFailed(null);
      setUnwrapping(false);
      setUnchecking(false);
      void queryClient.invalidateQueries({ queryKey: keys.plugins });
      void queryClient.invalidateQueries({ queryKey: keys.packages });
    },
    onError: (error, change) => {
      if (isApiError(error) && error.status === 409 && !change.on && !change.unwrapRuns) {
        setFailed(null);
        setUnwrapping(true);
        return;
      }
      setFailed(sayError(error, `${name} could not be turned ${change.on ? "on" : "off"}.`));
    },
  });
  const shown = turn.isPending && turn.variables ? turn.variables.on : enabled;
  return (
    <div className="flex flex-col items-end gap-2">
      <Switch
        checked={shown}
        label={`Plugin ${name}`}
        onChange={(on) => {
          if (turn.isPending) return;
          if (name === "guard" && !on) {
            setFailed(null);
            setUnchecking(true);
            return;
          }
          turn.mutate({ on, unwrapRuns: false });
        }}
      />
      {failed && !unwrapping && !unchecking && <Notice tone="error">{failed}</Notice>}
      <Dialog
        open={unchecking}
        onOpenChange={(open) => {
          if (!turn.isPending) {
            setUnchecking(open);
            if (!open) setFailed(null);
          }
        }}
      >
        <DialogContent className="gap-4 p-6">
          <DialogHeader>
            <DialogTitle>Turn off the guard?</DialogTitle>
            <DialogDescription>
              Without the guard, tool calls are no longer checked: neither the built-in safety
              patterns nor the rules apply, and every call is allowed. Turning it on again checks
              the calls that come after.
            </DialogDescription>
          </DialogHeader>
          {failed && <Notice tone="error">{failed}</Notice>}
          <DialogFooter>
            <Button
              variant="secondary"
              disabled={turn.isPending}
              onClick={() => {
                setUnchecking(false);
                setFailed(null);
              }}
            >
              Keep it on
            </Button>
            <Button
              variant="danger"
              disabled={turn.isPending}
              onClick={() => turn.mutate({ on: false, unwrapRuns: false, allowUnchecked: true })}
            >
              {turn.isPending ? "Turning off" : "Turn off, check nothing"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog
        open={unwrapping}
        onOpenChange={(open) => {
          if (!turn.isPending) {
            setUnwrapping(open);
            if (!open) setFailed(null);
          }
        }}
      >
        <DialogContent className="gap-4 p-6">
          <DialogHeader>
            <DialogTitle>Turn off the last launcher?</DialogTitle>
            <DialogDescription>
              {`${name} is the last launcher that is on. Without it, runs start unwrapped: nothing sandboxes them. Its entry and settings stay, and turning it on again wraps the runs that start after.`}
            </DialogDescription>
          </DialogHeader>
          {failed && <Notice tone="error">{failed}</Notice>}
          <DialogFooter>
            <Button
              variant="secondary"
              disabled={turn.isPending}
              onClick={() => {
                setUnwrapping(false);
                setFailed(null);
              }}
            >
              Keep it on
            </Button>
            <Button
              variant="danger"
              disabled={turn.isPending}
              onClick={() => turn.mutate({ on: false, unwrapRuns: true })}
            >
              {turn.isPending ? "Turning off" : "Turn off, run unwrapped"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
