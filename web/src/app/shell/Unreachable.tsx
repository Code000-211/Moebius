/** What the app shows when the server does not answer before anything was loaded: there is nothing to keep on screen. */
import { Button } from "../../ui/components/Button";
import { Loader } from "../../ui/components/Loader";

export function Unreachable({ retrying, onRetry }: { retrying: boolean; onRetry: () => void }) {
  return (
    <main className="flex min-h-full items-center justify-center bg-ground p-double">
      <div
        role="alert"
        className="flex w-full max-w-[440px] flex-col items-start gap-double rounded-xl border border-line bg-surface p-7"
      >
        <h1 className="text-[19px] font-bold text-high">Cannot reach the server</h1>
        <p className="text-sm text-normal">
          The server did not answer. Is it running? This page tries again by itself.
        </p>
        <div className="flex items-center gap-base">
          <Button onClick={onRetry} disabled={retrying}>
            Retry now
          </Button>
          {retrying && <Loader size={16} />}
        </div>
      </div>
    </main>
  );
}
