/**
 * Starts a real `moebius serve` for the end-to-end tests, when MOEBIUS_E2E_SERVER names the binary: in a new directory,
 * with a configuration that listens on a free port, and waits for the login link the server prints
 * (`http://<addr>/?login=<code>`) and for the token file. Needs the server of issue #143 (browser sign-in and the
 * embedded app); an older binary never prints the link and the start fails with what it printed.
 */
import { type ChildProcess, spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { FakeServer } from "./fakeServer";

/** A free port of the loopback address, found by binding port 0. */
function freePort(): Promise<number> {
  return new Promise((done, fail) => {
    const probe = createServer();
    probe.on("error", fail);
    probe.listen(0, "127.0.0.1", () => {
      const { port } = probe.address() as { port: number };
      probe.close(() => done(port));
    });
  });
}

export type RunningServer = Pick<FakeServer, "url" | "loginUrl" | "token"> & {
  stop(): Promise<void>;
  /** A real server is not restarted by the tests. */
  restart?: undefined;
  broadcast?: undefined;
  guardLog?: undefined;
};

export async function startRealServer(binary: string): Promise<RunningServer> {
  const directory = mkdtempSync(join(tmpdir(), "moebius-e2e-"));
  const port = await freePort();
  const routerPort = await freePort();
  writeFileSync(
    join(directory, "moebius.toml"),
    `listen = "127.0.0.1:${port}"\n\n[router]\nlisten = "127.0.0.1:${routerPort}"\n`,
  );
  const child: ChildProcess = spawn(binary, ["serve"], {
    cwd: directory,
    stdio: ["ignore", "pipe", "pipe"],
  });
  let output = "";
  const loginUrl = await new Promise<string>((done, fail) => {
    const timer = setTimeout(
      () => fail(new Error(`moebius serve printed no login link:\n${output}`)),
      60_000,
    );
    const read = (chunk: Buffer) => {
      output += chunk.toString("utf8");
      const found = /http:\/\/\S+\/\?login=\S+/.exec(output);
      if (found) {
        clearTimeout(timer);
        done(found[0]);
      }
    };
    child.stdout?.on("data", read);
    child.stderr?.on("data", read);
    child.on("exit", (code) => {
      clearTimeout(timer);
      fail(new Error(`moebius serve ended with ${code}:\n${output}`));
    });
  });
  const token = readFileSync(join(directory, ".moebius", "token"), "utf8").trim();
  const url = new URL(loginUrl).origin;
  return {
    url,
    loginUrl,
    token,
    async stop() {
      if (child.exitCode !== null || child.signalCode !== null) return;
      // The listener that failed the start goes first; removing it after would remove this one too.
      child.removeAllListeners("exit");
      const ended = new Promise<void>((done) => child.once("exit", () => done()));
      child.kill();
      await ended;
      rmSync(directory, { recursive: true, force: true });
    },
  };
}
