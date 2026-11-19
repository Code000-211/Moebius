/**
 * Opens what the desktop app asks the main window to show: the small window of the global shortcut sends `OPEN_EVENT`
 * with a path (`/chat/5`) once its question is sent or its issue made, and the router goes there, so the page keeps
 * what it holds. Nothing is drawn, and in a browser the event never comes.
 */
import { useEffect } from "react";
import { useNavigate } from "react-router";
import { OPEN_EVENT } from "./bridge";

/** Whether `path` is a page of the app: one `/` at its start, as the app checked before it sent it. */
function isLocal(path: unknown): path is string {
  return typeof path === "string" && path.startsWith("/") && !path.startsWith("//");
}

export function OpenRequests() {
  const navigate = useNavigate();
  useEffect(() => {
    const open = (event: Event) => {
      const path = (event as CustomEvent<unknown>).detail;
      if (isLocal(path)) navigate(path);
    };
    window.addEventListener(OPEN_EVENT, open);
    return () => window.removeEventListener(OPEN_EVENT, open);
  }, [navigate]);
  return null;
}
