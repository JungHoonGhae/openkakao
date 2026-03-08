import Image from 'next/image';
import Link from 'next/link';
import { cva } from 'class-variance-authority';
import { BatteryChargingIcon, FileIcon, SearchIcon, TerminalIcon, TimerIcon } from 'lucide-react';
import { cn } from '@/lib/cn';
import { CodeBlock } from '@/components/code-block';
import { CreateWorkflowAnimation, PreviewImages, WorkflowTabs } from '@/app/(home)/page.client';
import CLIImage from './cli.png';

const headingVariants = cva('font-medium tracking-tight', {
  variants: {
    variant: {
      h2: 'text-3xl lg:text-4xl',
      h3: 'text-xl lg:text-2xl',
    },
  },
});

const buttonVariants = cva(
  'inline-flex justify-center rounded-full px-5 py-3 font-medium tracking-tight transition-colors',
  {
    variants: {
      variant: {
        primary: 'bg-brand text-brand-foreground hover:bg-brand-200',
        secondary: 'border bg-fd-secondary text-fd-secondary-foreground hover:bg-fd-accent',
      },
    },
    defaultVariants: { variant: 'primary' },
  },
);

const cardVariants = cva('rounded-2xl bg-origin-border p-6 text-sm shadow-lg', {
  variants: {
    variant: {
      secondary: 'bg-brand-secondary text-brand-secondary-foreground',
      default: 'border bg-fd-card',
    },
  },
  defaultVariants: { variant: 'default' },
});

export default function Page() {
  return (
    <main className="pb-6 pt-4 text-landing-foreground dark:text-landing-foreground-dark md:pb-12">
      <div className="relative mx-auto flex h-[70vh] max-h-[900px] min-h-[600px] w-full max-w-[1400px] overflow-hidden rounded-2xl border bg-origin-border">
        <div className="absolute inset-0 bg-gradient-to-br from-brand/15 via-transparent to-brand-secondary/20 dark:from-brand/10 dark:to-brand-secondary/15" />
        <div className="z-2 flex size-full flex-col px-4 md:p-12 max-md:items-center max-md:text-center">
          <p className="mt-12 w-fit rounded-full border border-brand/50 p-2 text-xs font-medium text-brand">
            unofficial KakaoTalk CLI for macOS
          </p>
          <h1 className="my-8 text-4xl leading-tighter font-medium xl:mb-12 xl:text-5xl">
            Build practical KakaoTalk workflows,
            <br />
            your <span className="text-brand">way</span>.
          </h1>
          <div className="flex w-fit flex-row flex-wrap items-center justify-center gap-4">
            <Link href="/docs" className={cn(buttonVariants(), 'max-sm:text-sm')}>
              Getting Started
            </Link>
            <Link
              href="/docs/getting-started/quickstart"
              className={cn(buttonVariants({ variant: 'secondary' }), 'max-sm:text-sm')}
            >
              Quickstart
            </Link>
          </div>
        </div>
      </div>

      <div className="mx-auto mt-12 grid w-full max-w-[1400px] grid-cols-1 gap-10 px-6 md:px-12 lg:mt-20 lg:grid-cols-2">
        <p className="col-span-full text-2xl font-light leading-snug tracking-tight md:text-3xl xl:text-4xl">
          OpenKakao is a <span className="font-medium text-brand">local workflow surface</span> for
          developers who need to read chats, inspect history, watch live events, and move KakaoTalk
          context into tools they already control.
        </p>

        <div className="relative col-span-full overflow-hidden rounded-2xl p-4 md:p-8">
          <Image src={CLIImage} alt="" className="absolute inset-0 -z-1 size-full object-cover object-top" />
          <div className="mx-auto w-full max-w-[800px] rounded-2xl border bg-fd-card p-2 text-fd-card-foreground shadow-lg">
            <div className="flex flex-row gap-2">
              <h2 className="content-center rounded-xl border-2 border-brand/50 px-2 font-mono font-bold uppercase text-brand">
                Try it out
              </h2>
              <CodeBlock code="brew install openkakao-rs\nopenkakao-rs login --save" lang="bash" className="my-0 flex-1 bg-fd-secondary" />
            </div>

            <div className="relative mt-2 rounded-xl border bg-fd-secondary shadow-md">
              <div className="flex flex-row items-center gap-2 border-b p-2 text-fd-muted-foreground">
                <TerminalIcon className="size-4" />
                <span className="text-xs font-medium">Terminal</span>
                <div className="ms-auto me-2 size-2 rounded-full bg-red-400" />
              </div>
              <CreateWorkflowAnimation className="p-2 text-fd-secondary-foreground/80" />
            </div>
          </div>
        </div>

        <div className={cn(cardVariants({ variant: 'secondary', className: 'flex items-center justify-center p-0' }))}>
          <PreviewImages />
        </div>

        <div className={cn(cardVariants(), 'flex flex-col')}>
          <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-6' }))}>
            Official Fumadocs look, OpenKakao content.
          </h3>
          <p className="mb-4">
            The site now follows the same visual language as the main Fumadocs docs app while keeping
            the product story focused on OpenKakao.
          </p>
          <p className="mb-4">
            Start from the docs, then move into automation, trust boundaries, and protocol details only
            where you need them.
          </p>
          <CodeBlock
            code={'openkakao-rs loco-chats\nopenkakao-rs loco-read <chat_id> -n 20 --json\nopenkakao-rs watch --chat-id <chat_id>'}
            lang="bash"
            className="my-0"
          />
        </div>

        <WorkflowTabs
          tabs={{
            operator: (
              <div className="grid grid-cols-1 gap-8 lg:grid-cols-2">
                <CodeBlock
                  code={'openkakao-rs unread --json\nopenkakao-rs loco-read <chat_id> -n 50 --json | jq .'}
                  lang="bash"
                />
                <div className="max-lg:row-start-1">
                  <h3 className={cn(headingVariants({ variant: 'h3', className: 'my-4' }))}>Inspect first.</h3>
                  <p>Use OpenKakao to turn message streams into readable, reviewable input before you automate side effects.</p>
                  <ul className="mt-8 list-inside list-disc text-xs">
                    <li>Unread triage</li>
                    <li>History export</li>
                    <li>Review queues</li>
                    <li>Operator summaries</li>
                  </ul>
                </div>
              </div>
            ),
            developer: (
              <div className="grid grid-cols-1 gap-8 lg:grid-cols-2">
                <CodeBlock
                  code={'openkakao-rs watch --hook-cmd ./handle-event.sh\nopenkakao-rs watch --webhook-url https://hooks.example.com'}
                  lang="bash"
                />
                <div className="max-lg:row-start-1">
                  <h3 className={cn(headingVariants({ variant: 'h3', className: 'my-4' }))}>Compose your own stack.</h3>
                  <p>Use hooks, webhooks, JSON output, and local persistence to bridge KakaoTalk into the rest of your tooling.</p>
                  <ul className="mt-8 list-inside list-disc text-xs">
                    <li>Command hooks</li>
                    <li>Webhook delivery</li>
                    <li>SQLite and search indexes</li>
                    <li>CLI-first automation</li>
                  </ul>
                </div>
              </div>
            ),
            automation: (
              <div className="grid grid-cols-1 gap-8 lg:grid-cols-2">
                <CodeBlock
                  code={'1. read\n2. summarize\n3. classify\n4. review\n5. send only if needed'}
                  lang="txt"
                />
                <div className="max-lg:row-start-1">
                  <h3 className={cn(headingVariants({ variant: 'h3', className: 'my-4' }))}>Keep the boundary explicit.</h3>
                  <p>The project is useful because it stays close to the real app. That is also why the trust boundary has to stay explicit.</p>
                  <ul className="mt-8 list-inside list-disc text-xs">
                    <li>Local-first by default</li>
                    <li>REST and LOCO documented separately</li>
                    <li>Unattended mode is explicit</li>
                    <li>Outbound automation stays narrow</li>
                  </ul>
                </div>
              </div>
            ),
          }}
        />

        <div className={cn(cardVariants())}>
          <SearchIcon className="mb-3 size-8 text-brand" />
          <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-3' }))}>Search the docs</h3>
          <p>Move from high-level guidance into exact commands when you need the implementation details.</p>
          <Link href="/docs" className={cn(buttonVariants({ variant: 'primary', className: 'mt-6 w-fit text-sm py-2' }))}>
            Open documentation
          </Link>
        </div>

        <div className={cn(cardVariants())}>
          <TimerIcon className="mb-3 size-8 text-brand" />
          <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-3' }))}>Watch live events</h3>
          <p>Use watch mode when you need near-real-time reactions, and fallback to scheduled reads when a simpler failure model is better.</p>
          <Link href="/docs/cli/watch" className={cn(buttonVariants({ variant: 'secondary', className: 'mt-6 w-fit text-sm py-2' }))}>
            Watch mode
          </Link>
        </div>

        <div className={cn(cardVariants())}>
          <FileIcon className="mb-3 size-8 text-brand" />
          <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-3' }))}>Export and persist</h3>
          <p>Move selected chats into JSON, notes, local databases, or search indexes without inventing a hosted relay.</p>
          <Link href="/docs/automation/common-recipes" className={cn(buttonVariants({ variant: 'secondary', className: 'mt-6 w-fit text-sm py-2' }))}>
            Common recipes
          </Link>
        </div>

        <div className={cn(cardVariants())}>
          <BatteryChargingIcon className="mb-3 size-8 text-brand" />
          <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-3' }))}>Know the trust boundary</h3>
          <p>Understand what is read locally, what is stored, and when automation changes the privacy and account-safety model.</p>
          <Link href="/docs/security/trust-model" className={cn(buttonVariants({ variant: 'secondary', className: 'mt-6 w-fit text-sm py-2' }))}>
            Trust model
          </Link>
        </div>
      </div>
    </main>
  );
}
