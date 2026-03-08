export function getSection(path: string | undefined) {
  if (!path) return 'overview';

  const [dir] = path.split('/', 1);
  if (!dir) return 'overview';

  return (
    {
      overview: 'overview',
      'getting-started': 'getting-started',
      security: 'security',
      automation: 'automation',
      cli: 'cli',
      protocol: 'protocol',
    }[dir] ?? 'overview'
  );
}
