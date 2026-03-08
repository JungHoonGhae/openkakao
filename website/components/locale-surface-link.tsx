'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';

function fallbackHref(locale: 'en' | 'ko', surface: 'home' | 'docs') {
  if (surface === 'home') {
    return locale === 'ko' ? '/' : '/ko';
  }

  return locale === 'ko' ? '/docs' : '/ko/docs';
}

function switchLocalePath(pathname: string, locale: 'en' | 'ko', surface: 'home' | 'docs') {
  if (surface === 'home') {
    if (locale === 'ko') {
      if (pathname === '/ko' || pathname === '/ko/') return '/';
      if (pathname.startsWith('/ko/')) return pathname.slice(3) || '/';
      return fallbackHref(locale, surface);
    }

    if (pathname === '/' || pathname === '') return '/ko';
    return `/ko${pathname}`;
  }

  if (locale === 'ko') {
    if (pathname.startsWith('/ko/docs/')) return pathname.slice(3);
    if (pathname === '/ko/docs' || pathname === '/ko/docs/') return '/docs';
    return fallbackHref(locale, surface);
  }

  if (pathname.startsWith('/docs/')) return `/ko${pathname}`;
  if (pathname === '/docs' || pathname === '/docs/') return '/ko/docs';
  return fallbackHref(locale, surface);
}

export function LocaleSurfaceLink({
  locale,
  surface,
}: {
  locale: 'en' | 'ko';
  surface: 'home' | 'docs';
}) {
  const pathname = usePathname() ?? fallbackHref(locale, surface);
  const targetLabel = locale === 'ko' ? 'EN' : 'KO';
  const href = switchLocalePath(pathname, locale, surface);

  return (
    <Link
      href={href}
      className="inline-flex items-center justify-center rounded-full border border-zinc-300/80 bg-white px-3 py-1.5 text-xs font-semibold tracking-[0.18em] text-zinc-700 transition hover:border-zinc-400 hover:text-zinc-950 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200 dark:hover:border-zinc-500 dark:hover:text-zinc-50"
    >
      {targetLabel}
    </Link>
  );
}
