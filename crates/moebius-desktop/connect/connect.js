// The connect page: the address and the token go to the app's `connect` command, which checks the server and opens
// the web app in this window, or answers why not. The window's buttons call the bridge the app gives every page.
document.querySelectorAll("[data-window]").forEach((button) => {
  button.addEventListener("click", () => window.moebiusDesktop?.[button.dataset.window]());
});

const form = document.getElementById("connect");
const error = document.getElementById("error");
form.addEventListener("submit", async (event) => {
  event.preventDefault();
  const data = new FormData(form);
  const submit = form.querySelector("button[type=submit]");
  submit.disabled = true;
  error.hidden = true;
  try {
    await window.__TAURI_INTERNALS__.invoke("connect", {
      address: String(data.get("address")),
      token: String(data.get("token")),
    });
  } catch (reason) {
    error.textContent = String(reason);
    error.hidden = false;
    submit.disabled = false;
  }
});

try {
  const saved = localStorage.getItem("moebius.theme");
  const light =
    saved === "light" || (saved !== "dark" && matchMedia("(prefers-color-scheme: light)").matches);
  document.documentElement.dataset.theme = light ? "light" : "dark";
} catch {
  document.documentElement.dataset.theme = "dark";
}
