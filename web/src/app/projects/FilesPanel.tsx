/**
 * The Files box of a repository: the tree of its default branch on the left and, on the right, the files that were
 * opened as tabs, the one shown in an editor. The two columns are inset panels inside the card and are separated by
 * space alone. A tab with unsaved changes is marked and asks before it closes. Each column has its search bar above it:
 * the left one finds a file by its name and the right one finds text inside them, both as they are typed and with no
 * panel to open; Ctrl+P and Ctrl+Shift+F put the caret in one of them. The open files and their unsaved text are kept
 * while the page is shown.
 */
import { XIcon } from "@phosphor-icons/react";
import { useEffect, useRef, useState } from "react";
import { Button } from "../../ui/components/Button";
import { IconButton } from "../../ui/components/IconButton";
import { cn } from "../../ui/lib/cn";
import { isShortcut, modalIsOpen } from "../../ui/lib/shortcuts";
import { Box } from "../../ui/page/Box";
import { Empty } from "../../ui/page/Empty";
import { FileEditor } from "./FileEditor";
import { FileTree } from "./FileTree";
import { GoToFile, SearchInFiles } from "./FindPanels";
import { parents } from "./model";

export function FilesPanel({ repository }: { repository: number }) {
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set());
  const [tabs, setTabs] = useState<string[]>([]);
  const [current, setCurrent] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [line, setLine] = useState<{ path: string; line: number } | null>(null);
  /** The tab whose closing waits for the answer to "drop its changes?". */
  const [closing, setClosing] = useState<string | null>(null);
  const goField = useRef<HTMLInputElement>(null);
  const searchField = useRef<HTMLInputElement>(null);

  // Ctrl+P and Ctrl+Shift+F, as in an editor; the browser's own print and find give way while this page is shown. The
  // two bars are always there, so the shortcuts only have to move the caret into one of them. While a dialog is over
  // the page they would move it out of the dialog, so they stay with the browser.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (modalIsOpen()) return;
      if (isShortcut(e, "p")) {
        e.preventDefault();
        goField.current?.focus();
      } else if (isShortcut(e, "f", { shift: true })) {
        e.preventDefault();
        searchField.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const show = (path: string, at?: number) => {
    setTabs((list) => (list.includes(path) ? list : [...list, path]));
    setCurrent(path);
    setClosing(null);
    // The tree opens the directories above the file, so it is found there.
    setOpen((dirs) => new Set([...dirs, ...parents(path)]));
    setLine(at === undefined ? null : { path, line: at });
  };
  const close = (path: string) => {
    setTabs((list) => {
      const at = list.indexOf(path);
      const rest = list.filter((each) => each !== path);
      if (current === path) setCurrent(rest[Math.max(0, at - 1)] ?? null);
      return rest;
    });
    setDrafts((all) => {
      const { [path]: _dropped, ...rest } = all;
      return rest;
    });
    setClosing(null);
  };
  const toggle = (directory: string) =>
    setOpen((dirs) => {
      const next = new Set(dirs);
      if (!next.delete(directory)) next.add(directory);
      return next;
    });
  const unsaved = (path: string) => drafts[path] !== undefined;

  return (
    <Box title="Files" aside="Default branch" className="flex flex-col gap-4">
      {/* The tree stops where its last row stops: a box held to the height of the column beside it leaves a band of
          nothing under the files, which reads as a box that failed to fill. */}
      <div className="flex flex-col gap-4 md:flex-row md:items-start">
        <div className="flex w-full min-w-0 shrink-0 flex-col gap-2 md:w-64">
          <GoToFile repository={repository} onOpen={(path) => show(path)} fieldRef={goField} />
          <div className="max-h-96 w-full overflow-y-auto rounded-card bg-sunken p-2">
            <FileTree
              repository={repository}
              open={open}
              current={current}
              onToggle={toggle}
              onOpen={(path) => show(path)}
            />
          </div>
        </div>
        <div className="flex min-w-0 flex-1 flex-col gap-4">
          <SearchInFiles
            repository={repository}
            onOpen={(path, at) => show(path, at)}
            fieldRef={searchField}
          />
          {tabs.length === 0 ? (
            <Empty
              plain
              title="No file open"
              body="Pick a file on the left to read or edit it, or find one by its name above the tree. Search in files finds the text inside them."
            />
          ) : null}
          {closing !== null && (
            <div className="flex flex-wrap items-center gap-4 rounded-control bg-attention-fill p-3 text-attention-text">
              <p className="m-0 min-w-0 flex-1 break-words text-sm">
                {`${closing} has unsaved changes.`}
              </p>
              <Button size="sm" variant="secondary" onClick={() => setClosing(null)}>
                Keep
              </Button>
              <Button size="sm" variant="danger" onClick={() => close(closing)}>
                Discard
              </Button>
            </div>
          )}
          {current !== null && (
            <FileEditor
              key={current}
              repository={repository}
              path={current}
              draft={drafts[current]}
              tabs={
                <div
                  role="tablist"
                  aria-label="Open files"
                  className="flex min-w-0 flex-wrap gap-2"
                >
                  {tabs.map((path) => (
                    <div
                      key={path}
                      className={cn(
                        "flex min-h-10 items-center rounded-control",
                        path === current ? "bg-selected" : "bg-sunken hover:bg-hover",
                      )}
                    >
                      <button
                        type="button"
                        role="tab"
                        aria-selected={path === current}
                        title={path}
                        onClick={() => setCurrent(path)}
                        className={cn(
                          "min-h-10 min-w-0 max-w-64 break-words px-3 text-sm",
                          path === current ? "font-semibold text-text" : "text-secondary",
                        )}
                      >
                        {path.split("/").pop()}
                        {unsaved(path) && (
                          <span className="ml-2 text-xs font-semibold text-attention-text">
                            Unsaved
                          </span>
                        )}
                      </button>
                      <IconButton
                        icon={XIcon}
                        variant="ghost"
                        aria-label={`Close ${path}`}
                        onClick={() => (unsaved(path) ? setClosing(path) : close(path))}
                      />
                    </div>
                  ))}
                </div>
              }
              onDraft={(text) =>
                setDrafts((all) => {
                  if (text === undefined) {
                    const { [current]: _dropped, ...rest } = all;
                    return rest;
                  }
                  return { ...all, [current]: text };
                })
              }
              line={line?.path === current ? line.line : null}
              onLineShown={() => setLine(null)}
            />
          )}
        </div>
      </div>
    </Box>
  );
}
