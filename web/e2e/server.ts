/**
 * The server the end-to-end tests run against: the fake of `fakeServer.ts`, or, when MOEBIUS_E2E_SERVER is the path of a
 * `moebius` binary, a real `moebius serve` on a free port.
 */
import { type FakeServer, startFakeServer } from "./fakeServer";
import { startRealServer } from "./realServer";

export type TestServer = FakeServer | Awaited<ReturnType<typeof startRealServer>>;

export const usesRealServer = Boolean(process.env.MOEBIUS_E2E_SERVER);

export function startServer(): Promise<TestServer> {
  const binary = process.env.MOEBIUS_E2E_SERVER;
  return binary ? startRealServer(binary) : startFakeServer();
}
