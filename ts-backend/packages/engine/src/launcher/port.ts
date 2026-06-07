// launcher/port.ts — bind the first available loopback port (task 3.6).
// Transcribed from crates/stats-code/src/launcher/port.rs.
//
// Property 2: given an occupied set S ⊆ range, return the listener bound to
// min(range \ S); if range \ S is empty, throw AllPortsBusyError. We return the
// bound server (not just the number) to avoid a TOCTOU race where a third party
// grabs the port between scan and bind.

import net from 'node:net';

export interface ScanRange {
  /** inclusive start */
  start: number;
  /** exclusive end */
  endExclusive: number;
}

/** Stats_Code_Launcher default scan range 8080..8200. */
export const DEFAULT_RANGE: ScanRange = { start: 8080, endExclusive: 8200 };

export const LOOPBACK = '127.0.0.1';

export class AllPortsBusyError extends Error {
  readonly code = 'ALL_PORTS_BUSY';
  constructor(
    public readonly range: ScanRange,
    public readonly lastError: Error | null,
  ) {
    super(
      `all ports ${range.start}..${range.endExclusive} are busy` +
        (lastError ? ` (last error: ${lastError.message})` : ''),
    );
    this.name = 'AllPortsBusyError';
  }
}

/** Try to bind a single loopback port; resolves to the server or rejects. */
function tryBind(port: number): Promise<net.Server> {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once('error', (err) => {
      server.close();
      reject(err);
    });
    server.listen({ host: LOOPBACK, port, exclusive: true }, () => {
      resolve(server);
    });
  });
}

/**
 * Scan `range` in increasing order, returning a TCP server bound to the first
 * available loopback port. Throws AllPortsBusyError if none can bind.
 *
 * The returned server holds the port; callers can read the chosen port from
 * `(server.address() as net.AddressInfo).port` and either keep it for an HTTP
 * server handoff or close it.
 */
export async function scanFirstBindable(range: ScanRange = DEFAULT_RANGE): Promise<net.Server> {
  let lastError: Error | null = null;
  for (let port = range.start; port < range.endExclusive; port += 1) {
    try {
      return await tryBind(port);
    } catch (err) {
      lastError = err instanceof Error ? err : new Error(String(err));
    }
  }
  throw new AllPortsBusyError(range, lastError);
}

/** Probe whether a URL's host:port accepts a TCP connection within `timeoutMs`. */
export function isPortReachable(url: string, timeoutMs = 500): Promise<boolean> {
  return new Promise((resolve) => {
    const trimmed = url.replace(/\/+$/, '');
    const withoutScheme = trimmed.replace(/^https?:\/\//, '');
    const lastColon = withoutScheme.lastIndexOf(':');
    if (lastColon === -1) {
      resolve(false);
      return;
    }
    const host = withoutScheme.slice(0, lastColon);
    const port = Number.parseInt(withoutScheme.slice(lastColon + 1), 10);
    if (!Number.isInteger(port)) {
      resolve(false);
      return;
    }

    const socket = new net.Socket();
    let settled = false;
    const done = (result: boolean): void => {
      if (settled) return;
      settled = true;
      socket.destroy();
      resolve(result);
    };
    socket.setTimeout(timeoutMs);
    socket.once('connect', () => done(true));
    socket.once('timeout', () => done(false));
    socket.once('error', () => done(false));
    socket.connect(port, host);
  });
}
