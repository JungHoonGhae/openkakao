import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';

export const gitConfig = {
  user: 'JungHoonGhae',
  repo: 'openkakao',
  branch: 'main',
};

export function baseOptions(locale: 'en' | 'ko' = 'en'): BaseLayoutProps {
  const docsBase = locale === 'ko' ? '/ko/docs' : '/docs';
  const labels =
    locale === 'ko'
      ? {
          docs: '문서',
          security: '보안',
          automation: '자동화',
          protocol: '프로토콜',
          lang: 'English',
          langUrl: '/docs',
        }
      : {
          docs: 'Docs',
          security: 'Security',
          automation: 'Automation',
          protocol: 'Protocol',
          lang: '한국어',
          langUrl: '/ko/docs',
        };

  return {
    nav: {
      title: 'OpenKakao',
    },
    links: [
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
      {
        type: 'main',
        text: labels.automation,
        url: `${docsBase}/automation/overview`,
        active: 'nested-url',
      },
      {
        type: 'main',
        text: labels.protocol,
        url: `${docsBase}/protocol/overview`,
        active: 'nested-url',
      },
      {
        type: 'main',
        text: labels.lang,
        url: labels.langUrl,
        active: 'nested-url',
      },
    ],
    githubUrl: `https://github.com/${gitConfig.user}/${gitConfig.repo}`,
  };
}
