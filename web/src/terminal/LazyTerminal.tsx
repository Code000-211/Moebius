/**
 * xterm and its fit addon, fetched the first time a terminal is opened rather than with the page that may never show
 * one. Every place that shows a shell — a chat's own, a run's, a repository's, the Terminal page — uses this; the
 * component that holds xterm, [`Terminal`](./Terminal.tsx), is only ever imported here.
 */
import { type ComponentProps, lazy, Suspense } from "react";
import { Loader } from "../ui/components/Loader";

const TerminalPane = lazy(() => import("./Terminal").then((all) => ({ default: all.Terminal })));

export function Terminal(props: ComponentProps<typeof TerminalPane>) {
  return (
    <Suspense fallback={<Loader message="Opening the terminal" className="min-h-0 flex-1" />}>
      <TerminalPane {...props} />
    </Suspense>
  );
}
