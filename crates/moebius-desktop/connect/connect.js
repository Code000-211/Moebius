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

// The offer of a server on this machine: what the app finds is shown, and nothing is set up before the button is
// pressed. Declining is leaving the form below as it is.
const local = document.getElementById("local");
const localHint = document.getElementById("local-hint");
const localError = document.getElementById("local-error");
const localWorking = document.getElementById("local-working");
const localStart = document.getElementById("local-start");
const OFFERS = {
  "not-set-up": [
    "Moebius can run a server on this machine for you: the app downloads moebius of its own release (checked against " +
      "the release's SHA256SUMS) into ~/.moebius/bin, makes a configuration in ~/.moebius/server, starts the server " +
      "in the background and connects to it. Nothing is installed unless you ask for it here.",
    "Run a server on this machine",
  ],
  stopped: [
    "A server is set up in ~/.moebius/server and is not running. The app starts it and connects to it.",
    "Start it and connect",
  ],
  running: ["The server set up in ~/.moebius/server is running.", "Connect to it"],
};
window.__TAURI_INTERNALS__
  .invoke("local_state")
  .then((state) => {
    const offer = OFFERS[state];
    if (!offer) return;
    [localHint.textContent, localStart.textContent] = offer;
    local.hidden = false;
  })
  .catch(() => {});
localStart.addEventListener("click", async () => {
  localStart.disabled = true;
  localError.hidden = true;
  localWorking.hidden = false;
  try {
    await window.__TAURI_INTERNALS__.invoke("local_connect");
  } catch (reason) {
    localError.textContent = String(reason);
    localError.hidden = false;
    localStart.disabled = false;
  }
  localWorking.hidden = true;
});
