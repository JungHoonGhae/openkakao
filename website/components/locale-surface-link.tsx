import Link from 'next/link';

function localeHref(locale: 'en' | 'ko', surface: 'home' | 'docs') {
  if (surface === 'home') {
    return locale === 'ko' ? '/' : '/ko';
  }

  return locale === 'ko' ? '/docs' : '/ko/docs';
}

export function LocaleSurfaceLink({
  locale,
  surface,
}: {
  locale: 'en' | 'ko';
  surface: 'home' | 'docs';
}) {
  const targetLabel = locale === 'ko' ? 'EN' : 'KO';

  return (
    <Link
      href={localeHref(locale, surface)}
      className="inline-flex items-center justify-center rounded-full border border-zinc-300/80 bg-white px-3 py-1.5 text-xs font-semibold tracking-[0.18em] text-zinc-700 transition hover:border-zinc-400 hover:text-zinc-950 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200 dark:hover:border-zinc-500 dark:hover:text-zinc-50"
    >
      {targetLabel}
    </Link>
  );
}
