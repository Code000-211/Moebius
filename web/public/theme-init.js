// The colour theme before the first picture, so a light-system person never sees a dark flash.
// `src/theme.ts` owns the same key and keeps the theme in step with the system from here on.
// A file of its own and not an inline script, so that the server's content security policy needs no exception for it.
try {
  const saved = localStorage.getItem("moebius.theme");
  const light =
    saved === "light" || (saved !== "dark" && matchMedia("(prefers-color-scheme: light)").matches);
  document.documentElement.dataset.theme = light ? "light" : "dark";
} catch {
  document.documentElement.dataset.theme = "dark";
}
