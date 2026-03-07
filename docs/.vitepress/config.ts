import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'OpenKakao',
  description: 'Unofficial KakaoTalk CLI client for macOS',
  srcExclude: ['_archive/**'],
  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/favicon.svg' }]
  ],
  themeConfig: {
    logo: '/favicon.svg',
    nav: [
      { text: 'Guides', link: '/introduction' },
      { text: 'CLI Reference', link: '/cli/overview' },
      { text: 'Protocol', link: '/protocol/overview' },
    ],
    sidebar: {
      '/': [
        {
          text: 'Getting Started',
          items: [
            { text: 'Introduction', link: '/introduction' },
            { text: 'Quick Start', link: '/quickstart' },
            { text: 'Installation', link: '/installation' },
          ]
        },
        {
          text: 'Guides',
          items: [
            { text: 'Authentication', link: '/guides/authentication' },
            { text: 'Sending Messages', link: '/guides/sending-messages' },
            { text: 'Watching Messages', link: '/guides/watching-messages' },
            { text: 'Media Download', link: '/guides/media' },
            { text: 'Automation', link: '/guides/automation' },
          ]
        },
      ],
      '/cli/': [
        {
          text: 'CLI Reference',
          items: [
            { text: 'Overview', link: '/cli/overview' },
            { text: 'auth / login / relogin', link: '/cli/auth' },
            { text: 'chats / loco-chats', link: '/cli/chat' },
            { text: 'read / loco-read / export', link: '/cli/message' },
            { text: 'send / send-file', link: '/cli/send' },
            { text: 'watch', link: '/cli/watch' },
            { text: 'download', link: '/cli/media' },
            { text: 'friends / favorite', link: '/cli/friends' },
            { text: 'me / profile / settings', link: '/cli/profile' },
            { text: 'doctor / loco-test', link: '/cli/diagnostics' },
          ]
        }
      ],
      '/protocol/': [
        {
          text: 'LOCO Protocol',
          items: [
            { text: 'Overview', link: '/protocol/overview' },
            { text: 'Connection Flow', link: '/protocol/connection' },
            { text: 'Encryption', link: '/protocol/encryption' },
            { text: 'Packet Format', link: '/protocol/packets' },
            { text: 'Commands', link: '/protocol/commands' },
          ]
        }
      ],
    },
    socialLinks: [
      { icon: 'github', link: 'https://github.com/JungHoonGhae/openkakao' }
    ],
    search: {
      provider: 'local'
    },
    editLink: {
      pattern: 'https://github.com/JungHoonGhae/openkakao/edit/main/docs/:path'
    },
    footer: {
      message: 'Released under the MIT License.',
      copyright: 'Unofficial tool for technical research. Not affiliated with Kakao Corp.'
    }
  }
})
