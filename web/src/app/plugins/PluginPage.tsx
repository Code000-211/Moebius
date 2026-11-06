/**
 * A plugin's own page for its settings (`settings_ui` of its manifest), drawn in a plugin's box in place of the fields
 * `PluginSettingsBoxes` would draw. The plugin's module is not run in the app: it runs in a sandboxed frame
 * (`<iframe sandbox="allow-scripts">`, without `allow-same-origin`) that loads `public/plugin-frame.html`, so it has an
 * opaque origin, no cookies, no storage, no way to call the API as the person and no hold on the app's page; the frame
 * is also served with a policy that forbids it every network request. The app and the frame speak by messages only:
 * the app sends the module's text, the saved values, the app's stylesheets and the theme (and the theme again when it
 * changes), and the frame sends back values for the keys the plugin declares, which the app checks (`pageBridge`) and
 * saves as it saves the fields, and the height of its content, which sizes the frame so that it looks inline. A
 * message counts only when `event.source` is the frame's own window. When the module cannot be read, loaded or drawn,
 * or the frame says nothing for `PAGE_TIMEOUT_MS`, `onFailed` says why, so the box can draw the fields instead.
 * The page is drawn again from the saved values after a save or a revert.
 */
import { type JSX, useEffect, useRef, useState } from "react";
import { isApiError } from "../../api/http";
import type { PluginSetting } from "../../api/types/projects";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { appStyles, currentTheme, readFrameMessage } from "./pageBridge";
import { pageSource } from "./pageSource";

/** How long the frame has to draw the page before the fields are shown instead. */
export const PAGE_TIMEOUT_MS = 10_000;

/** What a plugin's page is handed: what the plugin declares and the values to start from. */
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
}

/** The part of the host that crosses into the frame: everything but the function. */
function given({ change: _change, ...rest }: SettingsHost) {
  return rest;
}

export function PluginPage({
  host,
  onFailed,
  timeoutMs = PAGE_TIMEOUT_MS,
}: {
  host: SettingsHost;
  onFailed: (why: string) => void;
  /** How long the frame has to draw the page; tests give a short time. */
  timeoutMs?: number;
}): JSX.Element {
  const frame = useRef<HTMLIFrameElement>(null);
  const [ready, setReady] = useState(false);
  const [height, setHeight] = useState(0);
  // The host as it was when the page was mounted: a change of values is reported by the page, not handed back to it.
  const first = useRef(host);
  const failed = useRef(onFailed);
  failed.current = onFailed;
  const limit = useRef(timeoutMs);
  useEffect(() => {
    let gone = false;
    let over = false;
    let frameReady = false;
    let loaded: { source: string; css: string } | null = null;
    let sent = false;
    const { plugin, version } = first.current;
    const fail = (why: string) => {
      if (gone || over) return;
      over = true;
      failed.current(why);
    };
    const post = (message: unknown) => frame.current?.contentWindow?.postMessage(message, "*");
    const begin = () => {
      if (sent || !frameReady || !loaded) return;
      sent = true;
      post({
        type: "init",
        css: loaded.css,
        theme: currentTheme(),
        source: loaded.source,
        host: given(first.current),
      });
    };
    const timer = setTimeout(() => fail("it did not draw in time"), limit.current);
    Promise.all([pageSource(plugin, version), appStyles()])
      .then(([source, css]) => {
        loaded = { source, css };
        begin();
      })
      .catch((error: unknown) =>
        fail(isApiError(error) || error instanceof Error ? error.message : String(error)),
      );
    const onMessage = (event: MessageEvent) => {
      // Only the frame this page made, and nothing else that has a window, is listened to.
      if (event.source === null || event.source !== frame.current?.contentWindow) return;
      const message = readFrameMessage(event.data, first.current.settings);
      if (!message) return;
      switch (message.type) {
        case "ready":
          frameReady = true;
          begin();
          break;
        case "mounted":
          clearTimeout(timer);
          if (!gone && !over) setReady(true);
          break;
        case "failed":
          clearTimeout(timer);
          fail(message.message);
          break;
        case "height":
          if (!gone) setHeight(message.height);
          break;
        case "change":
          if (!gone && !over) first.current.change(message.values);
          break;
      }
    };
    window.addEventListener("message", onMessage);
    // The frame follows the app's theme.
    const themes = new MutationObserver(() => post({ type: "theme", theme: currentTheme() }));
    themes.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => {
      gone = true;
      clearTimeout(timer);
      window.removeEventListener("message", onMessage);
      themes.disconnect();
    };
  }, []);
  return (
    <>
      {!ready && <SkeletonLines lines={3} message="Opening the plugin's page" />}
      <iframe
        ref={frame}
        title={`Settings page of ${host.plugin}`}
        data-testid="plugin-page"
        src="/plugin-frame.html"
        sandbox="allow-scripts"
        referrerPolicy="no-referrer"
        scrolling="no"
        className="block w-full min-w-0 border-0 bg-transparent"
        style={{ height: ready ? height : 0, colorScheme: "light dark" }}
      />
    </>
  );
}
