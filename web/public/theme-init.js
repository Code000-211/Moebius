// The colour theme before the first picture, so a dark-system person never sees a light flash. Light is the default:
// dark only when it was picked or the system asks for it.
// `src/theme.ts` owns the same key and keeps the theme in step with the system from here on.
// A file of its own and not an inline script, so that the server's content security policy needs no exception for it.
try {
  const saved = localStorage.getItem("moebius.theme");
  const dark =
    saved === "dark" || (saved !== "light" && matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
} catch {
  document.documentElement.dataset.theme = "light";
}
