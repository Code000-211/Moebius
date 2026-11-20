/**
 * Errors and confirmations as a toast at the bottom right of the panel: a solid surface that fades in (never slides) and
 * goes by itself after a delay. A confirmation of something that can be taken back offers Undo; an error has no Undo and
 * stays a little longer, because it is the one a person needs to read. The provider holds the toasts and the host draws
 * them; a page calls `useToast()` and never draws its own banner for a result (`docs/design-web.md`, Components).
 */
import { CheckCircleIcon, WarningCircleIcon, XIcon } from "@phosphor-icons/react";
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Button } from "../components/Button";
import { cn } from "../lib/cn";

/** How long a confirmation stays, in milliseconds. */
export const CONFIRMATION_MS = 5000;
/** How long an error stays: longer, because it is read and acted on. */
export const ERROR_MS = 8000;

export interface ToastOptions {
  kind: "error" | "confirmation";
  text: string;
  /** Takes back what a confirmation says was done. Called at most once; the toast goes with it. */
  undo?: () => void;
  /** Milliseconds until the toast goes; the default is `CONFIRMATION_MS` or `ERROR_MS`. */
  delay?: number;
}

interface Shown extends ToastOptions {
  id: number;
}

type ToastFn = (options: ToastOptions) => void;

const ToastContext = createContext<ToastFn>(() => {});

/** The function that shows a toast; where there is no provider it shows nothing. */
export function useToast(): ToastFn {
  return useContext(ToastContext);
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Shown[]>([]);
  const next = useRef(0);
  const dismiss = useCallback(
    (id: number) => setToasts((all) => all.filter((t) => t.id !== id)),
    [],
  );
  const toast = useCallback<ToastFn>((options) => {
    next.current += 1;
    const shown = { ...options, id: next.current };
    setToasts((all) => [...all, shown]);
  }, []);
  const host = useMemo(() => <ToastHost toasts={toasts} onDismiss={dismiss} />, [toasts, dismiss]);
  return (
    <ToastContext.Provider value={toast}>
      {children}
      {host}
    </ToastContext.Provider>
  );
}

function ToastHost({ toasts, onDismiss }: { toasts: Shown[]; onDismiss: (id: number) => void }) {
  return (
    <div
      aria-live="polite"
      data-toast-host=""
      className="pointer-events-none fixed right-4 bottom-4 z-50 flex w-96 max-w-[calc(100vw-2rem)] flex-col gap-2"
    >
      {toasts.map((t) => (
        <ToastItem key={t.id} toast={t} onDismiss={() => onDismiss(t.id)} />
      ))}
    </div>
  );
}

function ToastItem({ toast, onDismiss }: { toast: Shown; onDismiss: () => void }) {
  const error = toast.kind === "error";
  const delay = toast.delay ?? (error ? ERROR_MS : CONFIRMATION_MS);
  const dismiss = useRef(onDismiss);
  dismiss.current = onDismiss;
  useEffect(() => {
    const timer = setTimeout(() => dismiss.current(), delay);
    return () => clearTimeout(timer);
  }, [delay]);
  const Icon = error ? WarningCircleIcon : CheckCircleIcon;
  const { undo } = toast;
  return (
    <div
      role={error ? "alert" : "status"}
      className={cn(
        "pointer-events-auto flex items-start gap-2 rounded-control border border-hairline-strong bg-panel px-3 py-2 text-base text-text shadow-raised",
        "animate-in fade-in-0 duration-(--motion-routine) ease-out",
      )}
    >
      <Icon
        className={cn("mt-0.5 size-4 shrink-0", error ? "text-error-text" : "text-complete-text")}
        aria-hidden="true"
      />
      <span className="min-w-0 flex-1 break-words">{toast.text}</span>
      {undo && (
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            onDismiss();
            undo();
          }}
        >
          Undo
        </Button>
      )}
      <Button variant="icon" size="sm" aria-label="Dismiss" onClick={onDismiss}>
        <XIcon className="size-3.5" aria-hidden="true" />
      </Button>
    </div>
  );
}
