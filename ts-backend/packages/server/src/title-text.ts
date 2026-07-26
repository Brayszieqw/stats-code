// server/title-text.ts — shared sidebar-title text hygiene for session stores.
// Single source for MemSessionStore / FileSessionStore so the two derive-title
// paths cannot drift apart.

/** Drop controls / replacement chars that become "????" in sidebar titles. */
export function sanitizeTitleText(raw: string): string {
  const cleaned = [...raw]
    .filter((ch) => {
      const code = ch.codePointAt(0) ?? 0;
      if (code === 0xfffd) return false;
      if (code < 0x20 && code !== 0x09 && code !== 0x0a && code !== 0x0d) return false;
      return true;
    })
    .join('')
    .replace(/\s+/g, ' ')
    .trim();
  return cleaned;
}
