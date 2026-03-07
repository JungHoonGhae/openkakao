import type { Metadata } from 'next';
import { Manrope } from 'next/font/google';
import { Provider } from '@/components/provider';
import { gitConfig } from '@/lib/layout.shared';
import './global.css';

const manrope = Manrope({
  subsets: ['latin'],
});

const repoPagesUrl = `https://${gitConfig.user.toLowerCase()}.github.io/${gitConfig.repo}/`;
const basePath = process.env.NEXT_PUBLIC_BASE_PATH ?? '';

export const metadata: Metadata = {
  title: {
    default: 'OpenKakao Docs',
    template: '%s | OpenKakao Docs',
  },
  description: 'Unofficial KakaoTalk CLI client for macOS. Read, send, and automate personal chats from the terminal.',
  metadataBase: new URL(process.env.NEXT_PUBLIC_SITE_URL ?? repoPagesUrl),
  icons: {
    icon: `${basePath}/favicon.svg`,
  },
};

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <html lang="en" className={manrope.className} suppressHydrationWarning>
      <body className="flex flex-col min-h-screen">
        <Provider>{children}</Provider>
      </body>
    </html>
  );
}
