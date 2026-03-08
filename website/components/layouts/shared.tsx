import type { BaseLayoutProps, LinkItemType } from 'fumadocs-ui/layouts/shared';
import {
  BellRing,
  Book,
  Bot,
  Github,
  Search,
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
            <div className="-mx-3 -mt-3 overflow-hidden rounded-t-lg border-b bg-[#181818] p-3 text-white">
              <div className="mb-3 flex items-center gap-2 text-sm font-medium">
                <OpenKakaoIcon className="size-4 shrink-0" />
                OpenKakao
              </div>
              <div className="mb-3 flex items-center gap-2 rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-xs text-white/60">
                <Search className="size-3.5" />
                Search docs and commands
                <div className="ml-auto rounded border border-white/10 px-1.5 py-0.5 text-[10px]">⌘K</div>
              </div>
              <div className="grid grid-cols-[120px_1fr] gap-3">
                <div className="rounded-lg border border-white/10 bg-black/10 p-2 text-[11px] text-white/55">
                  <div className="mb-1 font-medium text-white/80">CLI</div>
                  <div className="rounded-md bg-white/5 px-2 py-1">Quickstart</div>
                  <div className="mt-1 px-2 py-1">Read / export</div>
                  <div className="px-2 py-1">Watch events</div>
                </div>
                <div className="rounded-lg border border-white/10 bg-white/4 p-3">
                  <div className="mb-2 text-xs font-medium uppercase tracking-wide text-white/45">Quickstart</div>
                  <div className="mb-2 text-sm font-medium text-white">From local app state to usable workflows.</div>
                  <div className="grid gap-2 text-[11px] text-white/60">
                    <div className="rounded-md border border-white/8 bg-white/4 px-2 py-1.5">Install and authenticate</div>
                    <div className="rounded-md border border-white/8 bg-white/4 px-2 py-1.5">Read recent chats as JSON</div>
                    <div className="rounded-md border border-white/8 bg-white/4 px-2 py-1.5">Watch events with local hooks</div>
                  </div>
                </div>
              </div>
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

          <NavbarMenuLink href="/docs/cli/overview" className="lg:col-start-2">
            <TerminalSquare className="bg-fd-primary text-fd-primary-foreground p-1 mb-2 rounded-md" />
            <p className="font-medium">CLI</p>
            <p className="text-fd-muted-foreground text-sm">
              Read, send, export, and watch with command-level control.
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

          <NavbarMenuLink href="/docs/automation/watch-patterns" className="lg:col-start-3 lg:row-start-2">
            <BellRing className="bg-fd-primary text-fd-primary-foreground p-1 mb-2 rounded-md" />
            <p className="font-medium">Watch Patterns</p>
            <p className="text-fd-muted-foreground text-sm">
              Trigger narrow event-driven flows without turning the CLI into a bus.
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
