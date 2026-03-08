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
    default: 'OpenKakao',
    template: '%s | OpenKakao',
  },
  description:
    'Bring KakaoTalk into local developer workflows. Read chats, watch events, export history, and build careful automations from macOS.',
  metadataBase: new URL(process.env.NEXT_PUBLIC_SITE_URL ?? repoPagesUrl),
  icons: {
    icon: `${basePath}/favicon.svg`,
  },
};

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <html lang="en" className={manrope.className} suppressHydrationWarning>
      <body className="flex min-h-screen flex-col">
        <Provider>{children}</Provider>
      </body>
    </html>
  );
}
