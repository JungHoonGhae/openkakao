'use client';

import { useParams } from 'next/navigation';
import type { ReactNode } from 'react';

function getSection(slug?: string): string | undefined {
  if (!slug) return;
  if (slug === 'cli') return 'cli';
  if (slug === 'protocol') return 'protocol';
  if (slug === 'security') return 'security';
  if (slug === 'automation') return 'automation';
  if (slug === 'getting-started') return 'getting-started';

  return;
}

export function Body({ children }: { children: ReactNode }) {
  const { slug = [] } = useParams();
  const mode = Array.isArray(slug) ? getSection(slug[0]) : undefined;

  return <body className={[mode, 'relative flex min-h-screen flex-col'].filter(Boolean).join(' ')}>{children}</body>;
}
