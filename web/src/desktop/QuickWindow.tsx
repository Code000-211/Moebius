/**
 * The small window the desktop app's global shortcut opens over everything (pattern 44, round 10, 10D): a person asks
 * the Moebius agent something, or makes an issue in a project, and Enter sends. The window then goes and the answer
 * (the chat) or the new issue (its board) opens in the app's main window. Shift+Enter is a new line, Escape closes
 * the window without sending. In an issue the first line is the title and the rest its description.
 *
 * It is a page of the web app of its own (`/quick`, outside the shell), shown by the app in a window without a frame.
 * The app keeps it hidden between uses, so each time it is shown again (`SHOWN_EVENT`) it starts empty with the cursor
 * in place; the project last chosen is remembered. The shortcut itself is changed here: Change, then the keys.
 */
import { useMutation, useQuery } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { createIssue } from "../api/calls/board";
import { startChat } from "../api/calls/chat";
import { listProjects } from "../api/calls/projects";
import { keys } from "../api/keys";
import type { Project } from "../api/types/projects";
import { boardPath } from "../app/pages";
import { ASSISTANT_AGENT } from "../chat/assistant";
import { Button } from "../ui/components/Button";
import { Select, TextArea } from "../ui/page/Field";
import { acceleratorOf } from "./accelerator";
import { DEFAULT_SHORTCUT, quickBridge, SHOWN_EVENT } from "./bridge";

type Mode = "ask" | "issue";

const PROJECT_KEY = "moebius.quick.project";

/** The project last chosen, when the browser keeps it. */
function rememberedProject(): string | null {
  try {
    return window.localStorage.getItem(PROJECT_KEY);
  } catch {
    return null;
  }
}

/** The repository that takes a project's new issues: its home, or the first it has. */
export function repositoryOf(project: Project): number | null {
  return (project.repositories.find((r) => r.home) ?? project.repositories[0])?.id ?? null;
}

/** The title and the description of an issue typed in one field: the first line, and what follows it. */
export function splitIssue(text: string): { title: string; body: string } {
  const trimmed = text.trim();
  const end = trimmed.indexOf("\n");
  return end < 0
    ? { title: trimmed, body: "" }
    : { title: trimmed.slice(0, end).trim(), body: trimmed.slice(end + 1).trim() };
}

/** Closes the window and shows `path` in the app; outside the app the page itself goes there. */
async function finish(path?: string) {
  const quick = quickBridge();
  if (quick) await quick.finish(path);
  else if (path) window.location.assign(path);
}

export function QuickWindow() {
  const field = useRef<HTMLTextAreaElement>(null);
  const [mode, setMode] = useState<Mode>("ask");
  const [text, setText] = useState("");
  const [chosen, setChosen] = useState<string | null>(rememberedProject);
  const [error, setError] = useState<string | null>(null);
  const list = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  const usable = (list.data ?? []).filter((p) => repositoryOf(p) !== null);
  const project = usable.find((p) => String(p.id) === chosen) ?? usable[0];

  const reset = useCallback(() => {
    setText("");
    setError(null);
    field.current?.focus();
  }, []);
  useEffect(() => {
    window.addEventListener(SHOWN_EVENT, reset);
    field.current?.focus();
    return () => window.removeEventListener(SHOWN_EVENT, reset);
  }, [reset]);

  // Escape leaves without sending; while a new shortcut is being recorded it only leaves the recording (which stops it).
  useEffect(() => {
    const leave = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !e.isComposing) void finish();
    };
    window.addEventListener("keydown", leave);
    return () => window.removeEventListener("keydown", leave);
  }, []);

  const send = useMutation({
    mutationFn: async (): Promise<string> => {
      if (mode === "ask") {
        const run = await startChat({
          agent: ASSISTANT_AGENT,
          message: text.trim(),
          origin: "assistant",
        });
        return `/chat/${run.id}`;
      }
      const repository = project ? repositoryOf(project) : null;
      if (repository === null) throw new Error("There is no project to make the issue in.");
      const { title, body } = splitIssue(text);
      const card = await createIssue(repository, title, body);
      return `${boardPath(repository)}?card=${card.id}`;
    },
    onSuccess: async (path) => {
      setText("");
      await finish(path);
    },
    onError: (e) => setError(e instanceof Error ? e.message : "Couldn't send that. Try again."),
  });

  const ready = text.trim() !== "" && !send.isPending;

  return (
    <div
      data-testid="quick-window"
      className="flex h-screen flex-col gap-2 bg-raised p-3 text-text"
    >
      <div data-tauri-drag-region className="flex items-center gap-2 text-sm">
        <div role="tablist" aria-label="What to do" className="flex gap-1">
          {(["ask", "issue"] as const).map((each) => (
            <Button
              key={each}
              type="button"
              role="tab"
              aria-selected={mode === each}
              variant={mode === each ? "secondary" : "ghost"}
              size="sm"
              onClick={() => {
                setMode(each);
                setError(null);
                field.current?.focus();
              }}
            >
              {each === "ask" ? "Ask Moebius" : "New issue"}
            </Button>
          ))}
        </div>
        {mode === "issue" && usable.length > 0 && (
          <Select
            aria-label="Project"
            className="w-48"
            value={String(project?.id ?? "")}
            options={usable.map((p): [string, string] => [String(p.id), p.name])}
            onChange={(value) => {
              setChosen(value);
              try {
                window.localStorage.setItem(PROJECT_KEY, value);
              } catch {
                // A browser that keeps nothing asks again next time.
              }
            }}
          />
        )}
      </div>
      <TextArea
        ref={field}
        aria-label={mode === "ask" ? "Ask Moebius" : "The new issue"}
        placeholder={
          mode === "ask" ? "Ask Moebius anything" : "Title on the first line, then what it is about"
        }
        className="min-h-0 flex-1 resize-none"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          // Enter sends; a new line is Shift+Enter, and a key that composes text is not Enter yet.
          if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
            e.preventDefault();
            if (ready) send.mutate();
          }
        }}
      />
      {error && (
        <p role="alert" className="text-error-text text-xs">
          {error}
        </p>
      )}
      <div className="flex items-center justify-between gap-2 text-secondary text-xs">
        <ShortcutSetting />
        <span>
          {send.isPending ? "Sending…" : "Enter sends · Shift+Enter new line · Esc closes"}
        </span>
      </div>
    </div>
  );
}

/** The shortcut that opens the window, and the way to change it: press Change, then the keys. */
function ShortcutSetting() {
  const quick = quickBridge();
  const [shortcut, setShortcut] = useState(DEFAULT_SHORTCUT);
  const [problem, setProblem] = useState<string | null>(null);
  const [recording, setRecording] = useState(false);

  useEffect(() => {
    let live = true;
    void quick?.shortcut().then((now) => {
      if (!live) return;
      setShortcut(now.shortcut);
      setProblem(now.error);
    });
    return () => {
      live = false;
    };
  }, [quick]);

  useEffect(() => {
    if (!recording || !quick) return;
    const listen = (e: KeyboardEvent) => {
      e.preventDefault();
      // Stopping it here keeps Escape from closing the window while it only leaves the recording.
      e.stopPropagation();
      if (e.key === "Escape") return setRecording(false);
      const next = acceleratorOf(e);
      if (next === null) return;
      setRecording(false);
      quick
        .setShortcut(next)
        .then((kept) => {
          setShortcut(kept);
          setProblem(null);
        })
        .catch((reason) => setProblem(String(reason)));
    };
    window.addEventListener("keydown", listen, true);
    return () => window.removeEventListener("keydown", listen, true);
  }, [recording, quick]);

  if (!quick) return <span />;
  return (
    <span className="flex min-w-0 items-center gap-2">
      <span className="truncate" title={problem ?? undefined}>
        {recording ? "Press the new keys…" : problem ? `${shortcut}: ${problem}` : shortcut}
      </span>
      <Button type="button" variant="ghost" size="sm" onClick={() => setRecording((on) => !on)}>
        {recording ? "Cancel" : "Change"}
      </Button>
    </span>
  );
}
