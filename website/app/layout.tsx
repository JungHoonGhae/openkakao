import type { Metadata } from 'next';
import type { Viewport } from 'next';
import { Geist, Geist_Mono } from 'next/font/google';
import { Provider } from '@/components/provider';
import { Body } from '@/app/layout.client';
import { gitConfig } from '@/components/layouts/shared';
import { source } from '@/lib/source';
import { NextProvider } from 'fumadocs-core/framework/next';
import { TreeContextProvider } from 'fumadocs-ui/contexts/tree';
import './global.css';

const geist = Geist({
  variable: '--font-sans',
  subsets: ['latin'],
});

const mono = Geist_Mono({
  variable: '--font-mono',
  subsets: ['latin'],
});

const repoPagesUrl = `https://${gitConfig.user.toLowerCase()}.github.io/${gitConfig.repo}/`;
const basePath = process.env.NEXT_PUBLIC_BASE_PATH ?? '';

export const metadata: Metadata = {
  title: {
    default: 'OpenKakao',
    template: '%s | OpenKakao',
  },
  description:
    'OpenKakao is an unofficial KakaoTalk CLI for macOS. Read chats, inspect history, watch events, and build local workflows from the terminal.',
  metadataBase: new URL(process.env.NEXT_PUBLIC_SITE_URL ?? repoPagesUrl),
  icons: {
    icon: `${basePath}/favicon.svg`,
  },
};

export const viewport: Viewport = {
  themeColor: [
    { media: '(prefers-color-scheme: dark)', color: '#0A0A0A' },
    { media: '(prefers-color-scheme: light)', color: '#fff' },
  ],
};

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <html lang="en" className={`${geist.variable} ${mono.variable}`} suppressHydrationWarning>
      <Body>
        <NextProvider>
          <TreeContextProvider tree={source.getPageTree()}>
            <Provider>{children}</Provider>
          </TreeContextProvider>
        </NextProvider>
      </Body>
    </html>
  );
}
