// launcher/job_object.ts — Win32 Job Object shim via koffi FFI (task 3.7).
//
// Mirrors crates/stats-code/src/launcher/process_guard.rs: create an anonymous
// Job Object, set JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, and assign child
// processes to it. When the parent process exits (including hard kill), the
// kernel closes the job handle and terminates every assigned process.
//
// koffi is pure FFI (no compiled addon) so it stays SEA-compatible, but it is
// an OPTIONAL dependency: if it is not installed, not on Windows, or any FFI
// call fails, createJobObject returns null and the ChildSupervisor falls back
// to its signal-based cleanup (design.md §"Process lifecycle", layer 2).

import { createRequire } from 'node:module';
import type { JobObjectHandle } from './supervisor.js';

const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x2000;
const JobObjectExtendedLimitInformation = 9;

// JOBOBJECT_EXTENDED_LIMIT_INFORMATION is 112 bytes on x64; LimitFlags sits at
// offset 16 (after the 8-byte PerProcessUserTimeLimit + 8-byte
// PerJobUserTimeLimit inside BASIC_LIMIT_INFORMATION).
const EXTENDED_LIMIT_INFO_SIZE = 112;
const LIMIT_FLAGS_OFFSET = 16;

interface KoffiLike {
  load(name: string): {
    func(signature: string): (...args: unknown[]) => unknown;
  };
}

/**
 * Create a Win32 Job Object with kill-on-close semantics, or return null if
 * the platform/FFI is unavailable. Never throws.
 */
export function createJobObject(): JobObjectHandle | null {
  if (process.platform !== 'win32') {
    return null;
  }

  let koffi: KoffiLike;
  try {
    const require = createRequire(import.meta.url);
    koffi = require('koffi') as KoffiLike;
  } catch {
    return null; // koffi not installed → caller falls back to signals
  }

  try {
    const kernel32 = koffi.load('kernel32.dll');
    const CreateJobObjectW = kernel32.func('void* __stdcall CreateJobObjectW(void*, void*)');
    const SetInformationJobObject = kernel32.func(
      'int __stdcall SetInformationJobObject(void*, int, void*, uint32)',
    );
    const OpenProcess = kernel32.func('void* __stdcall OpenProcess(uint32, int, uint32)');
    const AssignProcessToJobObject = kernel32.func(
      'int __stdcall AssignProcessToJobObject(void*, void*)',
    );
    const CloseHandle = kernel32.func('int __stdcall CloseHandle(void*)');

    const PROCESS_SET_QUOTA = 0x0100;
    const PROCESS_TERMINATE = 0x0001;

    const job = CreateJobObjectW(null, null) as unknown;
    if (!job) {
      return null;
    }

    // Set KILL_ON_JOB_CLOSE on the extended limit information.
    const info = Buffer.alloc(EXTENDED_LIMIT_INFO_SIZE);
    info.writeUInt32LE(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, LIMIT_FLAGS_OFFSET);
    const ok = SetInformationJobObject(
      job,
      JobObjectExtendedLimitInformation,
      info,
      EXTENDED_LIMIT_INFO_SIZE,
    ) as number;
    if (!ok) {
      CloseHandle(job);
      return null;
    }

    let closed = false;
    return {
      assign(pid: number): boolean {
        if (closed) return false;
        const handle = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid) as unknown;
        if (!handle) {
          return false;
        }
        const assigned = AssignProcessToJobObject(job, handle) as number;
        CloseHandle(handle);
        return assigned !== 0;
      },
      close(): void {
        if (closed) return;
        closed = true;
        CloseHandle(job);
      },
    };
  } catch {
    return null;
  }
}
