import type { BaseLayoutProps, LinkItemType } from 'fumadocs-ui/layouts/shared';
import { LocaleSurfaceLink } from '@/components/locale-surface-link';

export const gitConfig = {
  user: 'JungHoonGhae',
  repo: 'openkakao',
  branch: 'main',
};

function localeHref(locale: 'en' | 'ko', surface: 'home' | 'docs') {
  if (surface === 'home') {
    return locale === 'ko' ? '/' : '/ko';
  }

  return locale === 'ko' ? '/docs' : '/ko/docs';
}

export function baseOptions(
  locale: 'en' | 'ko' = 'en',
  surface: 'home' | 'docs' = 'docs',
): BaseLayoutProps {
  const docsBase = locale === 'ko' ? '/ko/docs' : '/docs';
  const labels =
    locale === 'ko'
      ? {
          useCases: '활용 사례',
          docs: '문서',
          security: '보안',
        }
      : {
          useCases: 'Use Cases',
          docs: 'Docs',
          security: 'Security',
        };

  const links: LinkItemType[] = [
    {
      type: 'main',
      text: labels.useCases,
      url: `${docsBase}/automation/overview`,
      active: 'nested-url',
    },
    {
      type: 'main',
      text: labels.docs,
      url: docsBase,
      active: 'nested-url',
    },
    {
      type: 'main',
      text: labels.security,
      url: `${docsBase}/security/trust-model`,
      active: 'nested-url',
    },
  ];

  if (surface === 'home') {
    links.push({
      type: 'button',
      text: locale === 'ko' ? 'EN' : 'KO',
      url: localeHref(locale, surface),
      active: 'none',
      secondary: true,
    });
  }

  return {
    nav: {
      title: 'OpenKakao',
      url: locale === 'ko' ? '/ko' : '/',
      transparentMode: surface === 'home' ? 'top' : 'none',
    },
    links,
    githubUrl: `https://github.com/${gitConfig.user}/${gitConfig.repo}`,
  };
}

export function docsFooter(locale: 'en' | 'ko') {
  return (
    <div className="flex items-center gap-2 pt-2">
      <span className="text-xs uppercase tracking-[0.18em] text-zinc-500 dark:text-zinc-400">
        {locale === 'ko' ? '언어' : 'Language'}
      </span>
      <LocaleSurfaceLink locale={locale} surface="docs" />
    </div>
  );
}
