import type { BaseLayoutProps, LinkItemType } from 'fumadocs-ui/layouts/shared';
import {
  Book,
  Bot,
  Github,
  ShieldCheck,
  TerminalSquare,
} from 'lucide-react';
import {
  NavbarMenu,
  NavbarMenuContent,
  NavbarMenuLink,
  NavbarMenuTrigger,
} from 'fumadocs-ui/layouts/home/navbar';
import Link from 'fumadocs-core/link';
import Image from 'next/image';
import Preview from '@/app/(home)/main.png';
import { OpenKakaoIcon } from '@/app/layout.client';

export const gitConfig = {
  user: 'JungHoonGhae',
  repo: 'openkakao',
  branch: 'main',
};

export const linkItems: LinkItemType[] = [
  {
    type: 'custom',
    on: 'nav',
    children: (
      <NavbarMenu>
        <NavbarMenuTrigger>
          <Link href="/docs">Documentation</Link>
        </NavbarMenuTrigger>
        <NavbarMenuContent>
          <NavbarMenuLink href="/docs" className="md:row-span-2">
            <div className="-mx-3 -mt-3">
              <Image
                src={Preview}
                alt="OpenKakao docs"
                className="rounded-t-lg object-cover"
                style={{
                  maskImage: 'linear-gradient(to bottom,white 60%,transparent)',
                }}
              />
            </div>
            <p className="font-medium">Documentation</p>
            <p className="text-fd-muted-foreground text-sm">
              Guides, reference, and trust boundaries for OpenKakao.
            </p>
          </NavbarMenuLink>

          <NavbarMenuLink href="/docs/getting-started/quickstart" className="lg:col-start-2">
            <Book className="bg-fd-primary text-fd-primary-foreground p-1 mb-2 rounded-md" />
            <p className="font-medium">Quickstart</p>
            <p className="text-fd-muted-foreground text-sm">
              Install, authenticate, and read your first chat.
            </p>
          </NavbarMenuLink>

          <NavbarMenuLink href="/docs/automation/overview" className="lg:col-start-2">
            <Bot className="bg-fd-primary text-fd-primary-foreground p-1 mb-2 rounded-md" />
            <p className="font-medium">Automation</p>
            <p className="text-fd-muted-foreground text-sm">
              Compose local workflows from CLI primitives.
            </p>
          </NavbarMenuLink>

          <NavbarMenuLink href="/docs/security/trust-model" className="lg:col-start-3 lg:row-start-1">
            <ShieldCheck className="bg-fd-primary text-fd-primary-foreground p-1 mb-2 rounded-md" />
            <p className="font-medium">Trust Model</p>
            <p className="text-fd-muted-foreground text-sm">
              Know what is read locally and where the risk sits.
            </p>
          </NavbarMenuLink>
        </NavbarMenuContent>
      </NavbarMenu>
    ),
  },
  {
    text: 'Quickstart',
    url: '/docs/getting-started/quickstart',
    icon: <Book />,
    active: 'nested-url',
  },
  {
    text: 'CLI',
    url: '/docs/cli/overview',
    icon: <TerminalSquare />,
    active: 'nested-url',
  },
  {
    text: 'Trust',
    url: '/docs/security/trust-model',
    icon: <ShieldCheck />,
    active: 'nested-url',
  },
  {
    type: 'icon',
    url: `https://github.com/${gitConfig.user}/${gitConfig.repo}`,
    label: 'github',
    text: 'GitHub',
    icon: <Github />,
    external: true,
  },
];

export const logoIcon = <OpenKakaoIcon className="size-5 shrink-0" />;

export const logo = (
  <span className="inline-flex items-center gap-2">
    {logoIcon}
    <span className="font-medium">OpenKakao</span>
  </span>
);

export function baseOptions(): BaseLayoutProps {
  return {
    nav: {
      title: logo,
    },
  };
}
