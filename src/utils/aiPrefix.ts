export function isAiPrefix(input: string): boolean {
  const t = input.trim();
  return t.startsWith("?") || t.toLowerCase().startsWith("ai ");
}
