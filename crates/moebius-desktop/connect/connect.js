// The connect page: the address and the token go to the app's `connect` command, which checks the server and opens
// the web app in this window, or answers why not. The window's buttons call the bridge the app gives every page.
document.querySelectorAll("[data-window]").forEach((button) => {
  button.addEventListener("click", () => window.moebiusDesktop?.[button.dataset.window]());
});

const form = document.getElementById("connect");
const error = document.getElementById("error");
const submit = document.getElementById("connect-submit");
const sessionOnly = document.getElementById("connect-session");
const sessionHint = document.getElementById("connect-session-hint");

// Shows why connecting failed. A keychain that does not answer also offers to go on without it, for this session.
function fail(reason) {
  const text = String(reason);
  error.textContent = text;
  error.hidden = false;
  const keychain = text.includes("keychain does not answer");
  sessionOnly.hidden = !keychain;
  sessionHint.hidden = !keychain;
}

async function connect(session) {
  const data = new FormData(form);
  submit.disabled = true;
  sessionOnly.disabled = true;
  error.hidden = true;
  const label = submit.textContent;
  submit.textContent = session ? "Connecting…" : "Saving the token in the keychain…";
  try {
    await window.__TAURI_INTERNALS__.invoke("connect", {
      address: String(data.get("address")),
      token: String(data.get("token")),
      sessionOnly: session,
    });
  } catch (reason) {
    fail(reason);
  }
  submit.textContent = label;
  submit.disabled = false;
  sessionOnly.disabled = false;
}
form.addEventListener("submit", (event) => {
  event.preventDefault();
  void connect(false);
});
sessionOnly.addEventListener("click", () => {
  if (form.reportValidity()) void connect(true);
});

// Why the remembered servers did not open at the start, when the keychain did not answer.
window.__TAURI_INTERNALS__
  .invoke("startup_notice")
  .then((notice) => {
    if (notice) fail(notice);
  })
  .catch(() => {});

// The servers the app remembers (their tokens are in the system's keychain): a click opens one, or says why not.
(async () => {
  try {
    const servers = await window.__TAURI_INTERNALS__.invoke("servers");
    if (servers.length === 0) return;
    const list = document.getElementById("saved-list");
    for (const server of servers) {
      const item = document.createElement("li");
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = server.reachable ? server.name : `${server.name} (cannot be reached)`;
      button.addEventListener("click", async () => {
        error.hidden = true;
        try {
          await window.__TAURI_INTERNALS__.invoke("switch_server", { id: server.id });
        } catch (reason) {
          error.textContent = String(reason);
          error.hidden = false;
        }
      });
      item.append(button);
      list.append(item);
    }
    document.getElementById("saved").hidden = false;
  } catch {
    // Without the list the page is still the form.
  }
})();

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
const localSession = document.getElementById("local-session");
async function startLocal(session) {
  localStart.disabled = true;
  localSession.disabled = true;
  localError.hidden = true;
  localWorking.hidden = false;
  try {
    await window.__TAURI_INTERNALS__.invoke("local_connect", { sessionOnly: session });
  } catch (reason) {
    localError.textContent = String(reason);
    localError.hidden = false;
    localSession.hidden = !String(reason).includes("keychain does not answer");
  }
  localStart.disabled = false;
  localSession.disabled = false;
  localWorking.hidden = true;
}
localStart.addEventListener("click", () => void startLocal(false));
localSession.addEventListener("click", () => void startLocal(true));
