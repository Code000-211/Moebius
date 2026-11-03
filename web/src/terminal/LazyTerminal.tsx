/**
 * xterm and its fit addon, fetched the first time a terminal is opened rather than with the page that may never show
 * one. Every place that shows a shell — a chat's own, a run's, a repository's, the Terminal page — uses this; the
 * component that holds xterm, [`Terminal`](./Terminal.tsx), is only ever imported here.
 */
import { type ComponentProps, lazy, Suspense } from "react";
import { SkeletonLines } from "../ui/page/Skeleton";

const TerminalPane = lazy(() => import("./Terminal").then((all) => ({ default: all.Terminal })));

export function Terminal(props: ComponentProps<typeof TerminalPane>) {
  return (
    <Suspense
      fallback={
        <SkeletonLines
          lines={12}
          message="Opening the terminal"
          className="min-h-0 flex-1 content-start"
        />
      }
    >
      <TerminalPane {...props} />
    </Suspense>
  );
}
