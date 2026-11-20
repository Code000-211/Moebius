/** A desktop bridge for tests, where each call does nothing and the rail has no servers. */
import type { DesktopBridge } from "./bridge";

export function fakeBridge(): DesktopBridge {
  return {
    minimize: () => {},
    toggleMaximize: () => {},
    close: () => {},
    servers: async () => [],
    switchServer: async () => {},
    removeServer: async () => {},
    renameServer: async () => {},
    connect: async () => {},
    connectSsh: async () => {},
  };
}
