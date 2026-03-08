'use client';

import { useParams } from 'next/navigation';
import { useId, type ReactNode } from 'react';

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

export function OpenKakaoIcon(props: React.SVGProps<SVGSVGElement>) {
  const id = useId();

  return (
    <svg width="80" height="80" viewBox="0 0 180 180" {...props}>
      <circle
        cx="90"
        cy="90"
        r="89"
        fill={`url(#${id}-iconGradient)`}
        stroke="var(--color-fd-primary)"
        strokeWidth="1"
      />
      <defs>
        <linearGradient id={`${id}-iconGradient`} gradientTransform="rotate(45)">
          <stop offset="35%" stopColor="var(--color-fd-background)" />
          <stop offset="100%" stopColor="#FEE500" />
        </linearGradient>
      </defs>
    </svg>
  );
}
