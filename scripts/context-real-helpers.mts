/** Read an explicit harness option without consuming the following option token. */
export function argument(name: string, args = process.argv): string {
  const index = args.indexOf(name);
  const value = index >= 0 ? args[index + 1] : undefined;
  if (index < 0 || !value || value.startsWith('--')) throw new Error(`Required: ${name}`);
  return value;
}

/** Select representative roots with local relationships from decoded oracle metadata. */
export function relationshipRoots(items: Record<string, unknown>[]): Set<string> {
  const local = new Set(items.map(item => String(item.id).trim().toLowerCase()));
  const external = (id: string, source: unknown): boolean =>
    ['global', 'external'].includes(String(source).trim().toLowerCase()) ||
    id.includes('://') || /^(github|gitlab|jira|linear):/i.test(id);
  const matches = (value: unknown, source?: unknown): boolean =>
    typeof value === 'string' && !external(value, source) && local.has(value.trim().toLowerCase());
  const predicates = [
    (item: Record<string, unknown>): boolean => matches(item.parent),
    (item: Record<string, unknown>): boolean => Array.isArray(item.dependencies) &&
      item.dependencies.some((dep: Record<string, unknown>) => matches(dep.id, dep.source_kind)),
    (item: Record<string, unknown>): boolean => matches(item.blocked_by),
  ];
  const roots = new Set<string>();
  const sorted = [...items].sort((a, b) => String(a.id).localeCompare(String(b.id)));
  for (const predicate of predicates) {
    const selected = sorted.find(predicate);
    if (selected) roots.add(String(selected.id));
  }
  return roots;
}
