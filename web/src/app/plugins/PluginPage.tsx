/**
 * A plugin's own page for its settings (`settings_ui` of its manifest), drawn in a plugin's box in place of the fields
 * `PluginSettingsBoxes` would draw. The module comes from `GET /v1/plugins/{plugin}/settings-ui` and runs in the web
 * app with the person's session, as trusted as the plugin. Its default export is called with an element to draw in and
 * a `SettingsHost`, and may return a function that takes the page down. The page only edits the values: Save, Revert,
 * the check of the values and the record of the save stay the box's and the server's.
 * The page is drawn again from the saved values after a save or a revert.
 */
import { type JSX, lazy, Suspense, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { isApiError } from "../../api/http";
import type { PluginSetting } from "../../api/types/projects";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { loadPage } from "./loadPage";

const CodeEditor = lazy(() => import("../../ui/CodeEditor"));

/** What a plugin's page is handed: where it is drawn, what the plugin declares, and how to report its values. */
export interface SettingsHost {
  plugin: string;
  version: string;
  /** `app` on the Plugin settings page, `project` on a repository's page. */
  scope: "app" | "project";
  /** The repository's id in the `project` scope, else null. */
  repository: number | null;
  /** The settings the plugin declares in this scope, each with its saved value. */
  settings: PluginSetting[];
  /** The values to start from, by key. */
  values: Record<string, string>;
  /** Reports every value of the plugin's settings in this scope, by key; a key left out is emptied by a save. */
  change: (values: Record<string, string>) => void;
  /** Draws the web app's code editor in `element`; returns what takes it down. */
  codeEditor: (element: HTMLElement, editor: CodeEditorOptions) => () => void;
}

export interface CodeEditorOptions {
  /** The editor's name for a screen reader. */
  label: string;
  /** The language to highlight, such as `dockerfile`. */
  language?: string;
  value: string;
  onChange: (value: string) => void;
}

function codeEditor(element: HTMLElement, editor: CodeEditorOptions): () => void {
  // Each editor in an element of its own, so that one taken down late does not clear one drawn in its place.
  const own = document.createElement("div");
  element.append(own);
  const root = createRoot(own);
  root.render(
    <Suspense fallback={<SkeletonLines lines={6} message="Opening the editor" />}>
      <CodeEditor
        label={editor.label}
        language={editor.language ?? ""}
        value={editor.value}
        onChange={editor.onChange}
      />
    </Suspense>,
  );
  // Taken down after the page's own render, which React does not allow during it.
  return () =>
    queueMicrotask(() => {
      root.unmount();
      own.remove();
    });
}

/**
 * The plugin's page, mounted once. When the module cannot be loaded or drawn, `onFailed` says why, so the box can draw
 * the fields instead.
 */
export function PluginPage({
  host,
  onFailed,
}: {
  host: Omit<SettingsHost, "codeEditor">;
  onFailed: (why: string) => void;
}): JSX.Element {
  const element = useRef<HTMLDivElement>(null);
  const [ready, setReady] = useState(false);
  // The host as it was when the page was mounted: a change of values is reported by the page, not handed back to it.
  const first = useRef(host);
  const failed = useRef(onFailed);
  failed.current = onFailed;
  useEffect(() => {
    const target = element.current;
    if (!target) return;
    let takeDown: (() => void) | undefined;
    let gone = false;
    const { plugin, version } = first.current;
    loadPage(plugin, version)
      .then((mount) => {
        if (gone) return;
        takeDown = mount(target, { ...first.current, codeEditor }) ?? undefined;
        setReady(true);
      })
      .catch((error: unknown) => {
        if (gone) return;
        failed.current(isApiError(error) || error instanceof Error ? error.message : String(error));
      });
    return () => {
      gone = true;
      takeDown?.();
      target.replaceChildren();
    };
  }, []);
  return (
    <>
      {!ready && <SkeletonLines lines={3} message="Opening the plugin's page" />}
      <div ref={element} data-testid="plugin-page" className="min-w-0" />
    </>
  );
}
