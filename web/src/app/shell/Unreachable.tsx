/** What the app shows when the server does not answer before anything was loaded: there is nothing to keep on screen. */
import { Button } from "../../ui/components/Button";
import { Loader } from "../../ui/components/Loader";

export function Unreachable({ retrying, onRetry }: { retrying: boolean; onRetry: () => void }) {
  return (
    <main className="flex min-h-full items-center justify-center bg-background p-6">
      <div
        role="alert"
        className="flex w-full max-w-(--connect-width) flex-col gap-4 rounded-card bg-surface p-8 shadow-card"
      >
        <h1 className="m-0 text-xl font-semibold tracking-tight text-text">
          Cannot reach the server
        </h1>
        <p className="m-0 text-base leading-relaxed text-secondary">
          The server did not answer. Is it running? This page tries again by itself.
        </p>
        <div className="flex items-center gap-3">
          <Button onClick={onRetry} disabled={retrying}>
            Retry now
          </Button>
          {retrying && <Loader size={16} />}
        </div>
      </div>
    </main>
  );
}
