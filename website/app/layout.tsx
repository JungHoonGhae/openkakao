import type { Metadata } from 'next';
import { Inter } from 'next/font/google';
import { Provider } from '@/components/provider';
import { gitConfig } from '@/lib/layout.shared';
import './global.css';

const inter = Inter({
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

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <html lang="en" className={inter.className} suppressHydrationWarning>
      <body className="flex min-h-screen flex-col">
        <Provider>{children}</Provider>
      </body>
    </html>
  );
}
