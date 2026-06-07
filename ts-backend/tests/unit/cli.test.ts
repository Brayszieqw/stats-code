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

  it('hidden parity/replay route to subcommand mode', () => {
    expect(classifyInvocation(['stats-code', 'parity', '--filter', 'tableone']).mode).toBe('subcommand');
    expect(classifyInvocation(['stats-code', 'replay', 'snapshot.zip']).mode).toBe('subcommand');
  });

  it('unknown flags do not flip to subcommand', () => {
    expect(classifyInvocation(['stats-code', '--definitely-not-a-flag']).mode).toBe('launcher');
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

  it('--help prints usage listing the three public flags and exits 0', async () => {
    const { io, out } = captureIo();
    const code = await main(['stats-code', '--help'], { io });
    expect(code).toBe(0);
    const text = out.join('');
    expect(text).toBe(USAGE);
    expect(text).toContain('--no-browser');
    expect(text).toContain('--version');
    expect(text).toContain('--help');
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

  it('help text does not advertise statistical subcommands', () => {
    // Public usage must not name internal statistical subcommands.
    for (const sub of ['tableone', 'survival', 'power', 'parity', 'replay', 'workflow']) {
      expect(USAGE).not.toContain(sub);
    }
  });
});
