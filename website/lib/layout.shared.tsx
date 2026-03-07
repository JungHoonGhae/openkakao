import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';

export const gitConfig = {
  user: 'JungHoonGhae',
  repo: 'openkakao',
  branch: 'main',
};

export function baseOptions(): BaseLayoutProps {
  return {
    nav: {
      title: 'OpenKakao',
    },
    links: [
      {
        type: 'main',
        text: 'Docs',
        url: '/docs',
        active: 'nested-url',
      },
      {
        type: 'main',
        text: 'Security',
        url: '/docs/security/trust-model',
        active: 'nested-url',
      },
      {
        type: 'main',
        text: 'Automation',
        url: '/docs/automation/overview',
        active: 'nested-url',
      },
      {
        type: 'main',
        text: 'Protocol',
        url: '/docs/protocol/overview',
        active: 'nested-url',
      },
    ],
    githubUrl: `https://github.com/${gitConfig.user}/${gitConfig.repo}`,
  };
}
