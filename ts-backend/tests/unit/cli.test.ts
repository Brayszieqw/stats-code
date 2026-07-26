import { describe, it, expect } from 'vitest';
import {
  classifyInvocation,
  main,
  USAGE,
  VERSION,
  type CliIo,
} from '@stats-code/engine';

function captureIo(): { io: CliIo; out: string[]; err: string[] } {
  const out: string[] = [];
  const err: string[] = [];
  return { io: { stdout: (l) => out.push(l), stderr: (l) => err.push(l) }, out, err };
}

describe('classifyInvocation', () => {
  it('bare invocation → launcher, browser enabled', () => {
    expect(classifyInvocation(['stats-code'])).toEqual({
      mode: 'launcher',
      args: { noBrowser: false },
    });
  });

  it('--no-browser → launcher, browser disabled', () => {
    expect(classifyInvocation(['stats-code', '--no-browser'])).toEqual({
      mode: 'launcher',
      args: { noBrowser: true },
    });
  });

  it('--version → version mode', () => {
    expect(classifyInvocation(['stats-code', '--version']).mode).toBe('version');
  });

  it('--help → help mode', () => {
    expect(classifyInvocation(['stats-code', '--help']).mode).toBe('help');
  });

  it('known subcommand → subcommand mode', () => {
    expect(classifyInvocation(['stats-code', 'tableone'])).toEqual({
      mode: 'subcommand',
      name: 'tableone',
    });
  });

  it('hidden parity stays internal while replay is a public verification mode', () => {
    expect(classifyInvocation(['stats-code', 'parity', '--filter', 'tableone']).mode).toBe('subcommand');
    expect(classifyInvocation(['stats-code', 'replay', 'snapshot.zip'])).toEqual({
      mode: 'replay',
      args: ['snapshot.zip'],
    });
  });

  it('unknown flags are rejected instead of silently launching (D8)', () => {
    expect(classifyInvocation(['stats-code', '--definitely-not-a-flag'])).toEqual({
      mode: 'invalid_flag',
      flag: '--definitely-not-a-flag',
    });
    expect(classifyInvocation(['stats-code', '--replay', 'x.zip'])).toEqual({
      mode: 'invalid_flag',
      flag: '--replay',
    });
    // Subcommand flag namespaces are untouched.
    expect(classifyInvocation(['stats-code', 'replay', 'x.zip', '--sha256', 'abc']).mode).toBe('replay');
    expect(classifyInvocation(['stats-code', 'parity', '--filter', 'tableone']).mode).toBe('subcommand');
  });

  it('--help takes precedence over a subcommand token', () => {
    expect(classifyInvocation(['stats-code', 'tableone', '--help']).mode).toBe('help');
  });
});

describe('main', () => {
  it('--version prints version and exits 0', async () => {
    const { io, out } = captureIo();
    const code = await main(['stats-code', '--version'], { io });
    expect(code).toBe(0);
    expect(out.join('')).toBe(`${VERSION}\n`);
  });

  it('--help prints public launcher and replay usage and exits 0', async () => {
    const { io, out } = captureIo();
    const code = await main(['stats-code', '--help'], { io });
    expect(code).toBe(0);
    const text = out.join('');
    expect(text).toBe(USAGE);
    expect(text).toContain('--no-browser');
    expect(text).toContain('--version');
    expect(text).toContain('--help');
    expect(text).toContain('replay <snapshot.zip>');
  });

  it('--no-browser delegates to the launcher runner with noBrowser=true', async () => {
    const { io } = captureIo();
    let received: { noBrowser: boolean } | undefined;
    const code = await main(['stats-code', '--no-browser'], {
      io,
      runLauncher: async (args) => {
        received = args;
        return 0;
      },
    });
    expect(code).toBe(0);
    expect(received).toEqual({ noBrowser: true });
  });

  it('internal subcommands are not publicly exposed (usage error)', async () => {
    const { io, err } = captureIo();
    const code = await main(['stats-code', 'parity'], { io });
    expect(code).toBe(1);
    expect(err.join('')).toContain('internal subcommand');
  });

  it('replay delegates directly without invoking the launcher', async () => {
    const { io, out } = captureIo();
    let launcherCalled = false;
    let replayArgs: { snapshotPath: string; expectedSha256?: string } | undefined;
    const code = await main(['stats-code', 'replay', 'snapshot.zip', '--sha256', 'a'.repeat(64)], {
      io,
      runLauncher: async () => {
        launcherCalled = true;
        return 0;
      },
      runReplay: async (args) => {
        replayArgs = args;
        return {
          outcome: { stepsReplayed: 2 },
          archiveSha256: 'a'.repeat(64),
          archiveAnchored: true,
        };
      },
    });
    expect(code).toBe(0);
    expect(launcherCalled).toBe(false);
    expect(replayArgs).toEqual({ snapshotPath: 'snapshot.zip', expectedSha256: 'a'.repeat(64) });
    expect(out.join('')).toContain('Replay PASS');
  });

  it('--replay (unknown flag) errors with exit 2 and never starts the launcher (D8)', async () => {
    const { io, err } = captureIo();
    let launcherCalled = false;
    const code = await main(['stats-code', '--replay', 'x.zip'], {
      io,
      runLauncher: async () => {
        launcherCalled = true;
        return 0;
      },
    });
    expect(code).toBe(2);
    expect(launcherCalled).toBe(false);
    expect(err.join('')).toContain("unknown option '--replay'");
    expect(err.join('')).toContain('replay <snapshot.zip>');
  });

  it('replay reports usage failure when the archive path is missing', async () => {
    const { io, err } = captureIo();
    expect(await main(['stats-code', 'replay'], { io })).toBe(2);
    expect(err.join('')).toContain('Replay FAIL');
  });

  it('help text does not advertise statistical subcommands', () => {
    // Public usage must not name internal statistical subcommands.
    for (const sub of ['tableone', 'survival', 'power', 'parity', 'workflow']) {
      expect(USAGE.toLowerCase()).not.toMatch(new RegExp(`\\b${sub}\\b`));
    }
  });
});
