/*
 * The script of the frame a plugin's own page for its settings runs in (plugin-frame.html). It is a classic script on
 * purpose: the frame has an opaque origin, where a module script could not be fetched without CORS. It speaks to the
 * web app by messages only (see src/app/plugins/pageBridge.ts and docs/reference/plugin-api.md):
 *
 *   from the app:  { type: "init", css, theme, source, host }  and  { type: "theme", theme }
 *   to the app:    { type: "ready" }, { type: "mounted" }, { type: "failed", message },
 *                  { type: "height", height } and { type: "change", values }
 *
 * `source` is the plugin's module; its default export is called with the element to draw in and a host with `plugin`,
 * `version`, `scope`, `repository`, `settings`, `values`, `change(values)` and `codeEditor(element, options)`.
 */
(() => {
  "use strict";
  const app = window.parent;
  const root = document.getElementById("plugin-root");
  let started = false;
  let mounted = false;

  const send = (message) => app.postMessage(message, "*");
  const fail = (error) => {
    if (mounted) return;
    send({ type: "failed", message: error instanceof Error ? error.message : String(error) });
  };
  const setTheme = (theme) => {
    if (theme === "light" || theme === "dark") document.documentElement.dataset.theme = theme;
  };

  const reportHeight = () => send({ type: "height", height: Math.ceil(root.getBoundingClientRect().height) });

  /** A plain text area for a file of code: the app's editor cannot run in a frame without an origin. */
  const codeEditor = (element, options) => {
    const area = document.createElement("textarea");
    area.className = "plugin-code-editor";
    area.dataset.testid = "code-editor";
    area.setAttribute("data-testid", "code-editor");
    area.setAttribute("aria-label", String(options.label ?? "Code"));
    if (options.language) area.dataset.language = String(options.language);
    area.spellcheck = false;
    area.value = String(options.value ?? "");
    area.addEventListener("input", () => options.onChange(area.value));
    area.addEventListener("keydown", (event) => {
      if (event.key !== "Tab" || event.shiftKey || event.ctrlKey || event.metaKey || event.altKey) return;
      event.preventDefault();
      area.setRangeText("  ", area.selectionStart, area.selectionEnd, "end");
      options.onChange(area.value);
    });
    element.append(area);
    return () => area.remove();
  };

  const start = async (message) => {
    setTheme(message.theme);
    document.getElementById("app-css").textContent = String(message.css ?? "");
    const url = URL.createObjectURL(new Blob([String(message.source)], { type: "text/javascript" }));
    let module;
    try {
      module = await import(url);
    } finally {
      URL.revokeObjectURL(url);
    }
    if (typeof module.default !== "function") throw new Error("the page exports no default function");
    const given = message.host;
    const host = {
      plugin: given.plugin,
      version: given.version,
      scope: given.scope,
      repository: given.repository,
      settings: given.settings,
      values: given.values,
      change: (values) => {
        if (values === null || typeof values !== "object") throw new TypeError("change takes an object of values");
        send({ type: "change", values });
      },
      codeEditor,
    };
    await module.default(root, host);
    new ResizeObserver(reportHeight).observe(root);
    reportHeight();
    mounted = true;
    send({ type: "mounted" });
  };

  window.addEventListener("message", (event) => {
    if (event.source !== app) return;
    const message = event.data;
    if (message === null || typeof message !== "object") return;
    if (message.type === "init" && !started) {
      started = true;
      start(message).catch(fail);
    } else if (message.type === "theme") {
      setTheme(message.theme);
    }
  });
  window.addEventListener("error", (event) => fail(event.error ?? event.message));
  window.addEventListener("unhandledrejection", (event) => fail(event.reason));
  send({ type: "ready" });
})();
