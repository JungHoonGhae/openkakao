import Image from 'next/image';
import Link from 'next/link';
import { cva } from 'class-variance-authority';
import {
  BatteryChargingIcon,
  BellRing,
  BookIcon,
  FileIcon,
  SearchIcon,
  ShieldCheck,
  TerminalIcon,
  TimerIcon,
  Webhook,
} from 'lucide-react';
import { cn } from '@/lib/cn';
import { Marquee } from '@/app/(home)/marquee';
import { CodeBlock } from '@/components/code-block';
import {
  AgnosticBackground,
  CreateWorkflowAnimation,
  Hero,
  PreviewImages,
  Writing,
} from '@/app/(home)/page.client';
import CLIImage from './cli.png';
import Bg2Image from './bg-2.png';
import StoryImage from './story.png';
import MainImage from './main.png';

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

const feedback = [
  {
    title: 'Read',
    message: 'Pull recent cache-backed reads or full LOCO history depending on how much correctness the workflow needs.',
  },
  {
    title: 'Watch',
    message: 'Move from polling to event-driven workflows with reconnect-aware real-time monitoring.',
  },
  {
    title: 'Export',
    message: 'Push selected message slices into JSON, SQLite, search indexes, and local tools you already control.',
  },
  {
    title: 'Send carefully',
    message: 'Keep outbound actions narrow, explicit, and close to the operator rather than hiding them in a hosted relay.',
  },
];

export default function Page() {
  return (
    <main className="pb-6 pt-4 text-landing-foreground dark:text-landing-foreground-dark md:pb-12">
      <div className="relative mx-auto flex h-[70vh] max-h-[900px] min-h-[600px] w-full max-w-[1400px] overflow-hidden rounded-2xl border bg-origin-border">
        <Hero />
        <div className="z-2 flex size-full flex-col px-4 md:p-12 max-md:items-center max-md:text-center">
          <p className="mt-12 w-fit rounded-full border border-brand/50 p-2 text-xs font-medium text-brand">
            unofficial KakaoTalk CLI for macOS
          </p>
          <h1 className="my-8 text-4xl leading-tighter font-medium xl:mb-12 xl:text-5xl">
            Build excellent KakaoTalk workflows,
            <br />
            your <span className="text-brand">style</span>.
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
          OpenKakao opens a <span className="font-medium text-brand">real workflow surface</span>{' '}
          around KakaoTalk for developers and operators who need local reads, event monitoring, structured
          exports, and controlled automation without inventing a hosted relay.
        </p>

        <div className="relative col-span-full overflow-hidden rounded-2xl p-4 md:p-8">
          <Image src={CLIImage} alt="" className="absolute inset-0 -z-1 size-full object-cover object-top" />
          <div className="mx-auto w-full max-w-[800px] rounded-2xl border bg-fd-card p-2 text-fd-card-foreground shadow-lg">
            <div className="flex flex-row gap-2">
              <h2 className="content-center rounded-xl border-2 border-brand/50 px-2 font-mono font-bold uppercase text-brand">
                Try it out
              </h2>
              <CodeBlock
                code="brew install openkakao-rs\nopenkakao-rs login --save"
                lang="bash"
                className="my-0 flex-1 bg-fd-secondary"
              />
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

        <Feedback />
        <Aesthetics />
        <AnybodyCanWrite />
        <ForEngineers />
        <OpenSource />
      </div>
    </main>
  );
}

function Feedback() {
  return (
    <>
      <div className={cn(cardVariants())}>
        <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-6' }))}>
          A practical CLI surface.
        </h3>
        <p className="mb-6">
          Use OpenKakao where KakaoTalk already holds context, but the official developer workflow
          surface is still missing or too limited for personal automation.
        </p>
        <Link href="/docs/overview/why-openkakao" className={cn(buttonVariants())}>
          Why it exists
        </Link>
      </div>
      <div className={cn(cardVariants({ variant: 'secondary', className: 'relative p-0' }))}>
        <div className="absolute inset-0 z-2 rounded-2xl inset-shadow-[0_10px_60px] inset-shadow-brand-secondary" />
        <Marquee className="p-8">
          {feedback.map((item) => (
            <div
              key={item.title}
              className="flex w-[320px] flex-col rounded-xl border bg-fd-card p-4 text-landing-foreground shadow-lg"
            >
              <p className="text-sm font-medium">{item.title}</p>
              <p className="mt-3 text-sm whitespace-pre-wrap">{item.message}</p>
            </div>
          ))}
        </Marquee>
      </div>
    </>
  );
}

function Aesthetics() {
  return (
    <>
      <div className={cn(cardVariants({ variant: 'secondary', className: 'flex items-center justify-center p-0' }))}>
        <PreviewImages />
      </div>
      <div className={cn(cardVariants(), 'flex flex-col')}>
        <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-6' }))}>
          One docs shell, several workflow surfaces.
        </h3>
        <p className="mb-4">
          Move from guides to command reference to trust boundary without leaving the same visual
          system.
        </p>
        <p className="mb-4">
          The docs now follow the official Fumadocs landing language more closely while keeping the
          OpenKakao story focused on operator value.
        </p>
        <CodeBlock
          code={'openkakao-rs loco-chats\nopenkakao-rs loco-read <chat_id> -n 20 --json\nopenkakao-rs watch --chat-id <chat_id>'}
          lang="bash"
          className="my-0"
        />
      </div>
    </>
  );
}

function AnybodyCanWrite() {
  return (
    <Writing
      tabs={{
        operator: (
          <div className="grid grid-cols-1 gap-8 lg:grid-cols-2">
            <CodeBlock
              code={'openkakao-rs unread --json\nopenkakao-rs loco-read <chat_id> -n 50 --json | jq .'}
              lang="bash"
            />
            <div className="max-lg:row-start-1">
              <h3 className={cn(headingVariants({ variant: 'h3', className: 'my-4' }))}>
                Inspect first.
              </h3>
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
              code={'openkakao-rs watch --hook-cmd ./handle-event.sh\nopenkakao-rs watch --webhook-url https://hooks.example.com/openkakao'}
              lang="bash"
            />
            <div className="max-lg:row-start-1">
              <h3 className={cn(headingVariants({ variant: 'h3', className: 'my-4' }))}>
                Compose your own stack.
              </h3>
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
              <h3 className={cn(headingVariants({ variant: 'h3', className: 'my-4' }))}>
                Keep the boundary explicit.
              </h3>
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
  );
}

function StoryCard() {
  return (
    <div className="relative col-span-full min-h-[570px] rounded-2xl border px-2 py-6 shadow-md">
      <Image
        src={StoryImage}
        alt=""
        className="absolute inset-0 -z-1 size-full rounded-2xl object-cover object-top"
      />
      <div className="m-auto w-full max-w-[500px] rounded-xl border bg-fd-card/80 p-2 text-start shadow-xl shadow-black/20 backdrop-blur-md dark:bg-fd-card/50">
        <div className="px-3 pt-3">
          <h2 className={cn(headingVariants({ className: 'mb-4', variant: 'h3' }))}>Why this exists</h2>
          <p className="mb-4 text-sm">
            KakaoTalk already holds requests, updates, and coordination. OpenKakao exists because
            personal developer workflows around that context are still structurally limited.
          </p>
          <Link href="/docs/overview/why-openkakao" className={cn(buttonVariants({ className: 'mb-4 py-2 text-sm' }))}>
            Explore
          </Link>
        </div>
        <div className="rounded-xl border bg-fd-secondary p-4">
          <p className="text-sm font-medium">The value is composition, not one command.</p>
          <p className="mt-3 text-sm text-fd-muted-foreground">
            Read, export, watch, classify, review, and only then send if the workflow still needs it.
          </p>
        </div>
      </div>
    </div>
  );
}

function ForEngineers() {
  return (
    <>
      <h2 className={cn(headingVariants({ variant: 'h2', className: 'col-span-full mb-4 text-center text-brand' }))}>
        Docs For Operators.
      </h2>
      <StoryCard />

      <div className={cn(cardVariants(), 'relative z-2 flex flex-col overflow-hidden')}>
        <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-6' }))}>
          Two transport surfaces, one CLI.
        </h3>
        <p className="mb-20">
          REST stays cheap and cache-backed. LOCO handles real chat workflows, live monitoring, and
          sending. The docs make the boundary explicit instead of hiding it.
        </p>
        <div className="mt-auto flex w-fit flex-row gap-2 rounded-xl bg-brand p-2 text-brand-foreground">
          <div className="rounded-lg bg-black/10 px-3 py-2 text-sm font-medium">REST</div>
          <div className="rounded-lg bg-black/10 px-3 py-2 text-sm font-medium">LOCO</div>
          <div className="rounded-lg bg-black/10 px-3 py-2 text-sm font-medium">Hooks</div>
          <div className="rounded-lg bg-black/10 px-3 py-2 text-sm font-medium">Webhooks</div>
        </div>
        <AgnosticBackground />
      </div>

      <div className={cn(cardVariants(), 'flex flex-col')}>
        <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-6' }))}>
          Composable primitives.
        </h3>
        <p className="mb-8">
          The CLI is intentionally small: reads, watch, search, export, send, and authentication
          recovery. That keeps it scriptable without pretending to be a full automation platform.
        </p>
        <div className="mt-auto flex flex-col gap-2 @container mask-[linear-gradient(to_bottom,white,transparent)]">
          {[
            { name: 'read', description: 'Cheap cache-backed message reads when GUI recency is enough.' },
            { name: 'loco-read', description: 'Reliable history fetches for exports and automation.' },
            { name: 'watch', description: 'Real-time monitoring with reconnect handling and narrow side effects.' },
            { name: 'send', description: 'Controlled outbound messaging with explicit unattended policy.' },
            { name: 'login / relogin / renew', description: 'Recovery and credential reuse around the real app state.' },
          ].map((item) => (
            <div
              key={item.name}
              className="flex flex-col gap-2 border border-dashed border-brand-secondary p-2 text-sm @lg:flex-row @lg:items-center"
            >
              <p className="font-medium text-nowrap">{item.name}</p>
              <p className="text-xs @lg:flex-1 @lg:text-end">{item.description}</p>
            </div>
          ))}
        </div>
      </div>

      <div className={cn(cardVariants(), 'flex flex-col')}>
        <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-6' }))}>
          Adopts your local stack.
        </h3>
        <p className="mb-4">
          OpenKakao is strongest when it feeds tools you already trust: `jq`, `sqlite`, launchd, local
          agents, search indexes, and narrow webhook receivers.
        </p>
        <div className="mb-6 flex w-fit flex-row items-center gap-4">
          {['jq', 'sqlite', 'launchd', 'webhooks'].map((item) => (
            <span key={item} className="text-sm text-brand">
              {item}
            </span>
          ))}
        </div>
        <CodeBlock
          code={`openkakao-rs loco-read <chat_id> --all --json > history.json
jq '.[] | {author, message}' history.json
sqlite3 chat.db '.import history.json messages'`}
          lang="bash"
          className="my-0"
        />
      </div>

      <div className={cn(cardVariants({ className: 'relative min-h-[400px] overflow-hidden z-2' }))}>
        <Image src={Bg2Image} alt="" className="absolute inset-0 -z-1 size-full object-cover object-top" />
        <div className="absolute left-4 top-8 flex w-[70%] flex-col rounded-xl border bg-neutral-50/80 p-2 text-neutral-800 shadow-lg shadow-black backdrop-blur-lg dark:bg-neutral-900/80 dark:text-neutral-200">
          <p className="mb-2 border-b px-2 pb-2 font-medium text-neutral-500 dark:text-neutral-400">
            Local workflow
          </p>
          {['Unread review', 'JSON export', 'Webhook receiver', 'Operator summary'].map((page) => (
            <div key={page} className="flex items-center gap-2 rounded-lg p-2 hover:bg-neutral-400/20">
              <FileIcon className="size-4 stroke-neutral-500 dark:stroke-neutral-400" />
              <span className="text-sm">{page}</span>
              <div className="ms-auto rounded-full bg-brand px-3 py-1 font-mono text-xs text-brand-foreground">
                Step
              </div>
            </div>
          ))}
        </div>
        <div className="absolute bottom-8 right-4 flex w-[70%] flex-col rounded-xl border bg-neutral-100 text-neutral-800 shadow-lg shadow-black dark:bg-neutral-900 dark:text-neutral-200">
          <div className="border-b px-4 py-2 font-medium text-neutral-500 dark:text-neutral-400">
            CLI
          </div>
          <pre className="overflow-auto p-4 text-base text-neutral-800 dark:text-neutral-400">{`openkakao-rs unread --json
openkakao-rs watch --hook-cmd ./handle-event.sh
openkakao-rs send <chat_id> "done"`}</pre>
        </div>
      </div>

      <div className={cn(cardVariants(), 'flex flex-col max-md:pb-0')}>
        <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-6' }))}>
          Search the exact command surface.
        </h3>
        <p className="mb-6">
          The landing explains where the CLI helps. The docs search takes you to the exact command,
          flag, and risk boundary when you need to implement.
        </p>
        <Link href="/docs" className={cn(buttonVariants({ className: 'mb-8 w-fit' }))}>
          Open docs
        </Link>
        <SearchPanel />
      </div>

      <div className={cn(cardVariants(), 'flex flex-col overflow-hidden p-0')}>
        <div className="mb-2 p-6">
          <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-6' }))}>
            The workflow docs for OpenKakao
          </h3>
          <p className="mb-6">
            From quickstart to trust model to command-level detail, the site is now shaped like the
            official Fumadocs landing but grounded in OpenKakao's actual operator story.
          </p>
          <Link href="/docs/cli/overview" className={cn(buttonVariants({ className: 'w-fit' }))}>
            Command reference
          </Link>
        </div>
        <Image src={MainImage} alt="OpenKakao docs preview" className="mt-auto w-full flex-1 object-cover" />
      </div>
    </>
  );
}

function SearchPanel() {
  const items = [
    ['Quickstart', 'Install, authenticate, and read your first chat.'],
    ['read / loco-read', 'Choose between cache-backed reads and full history fetches.'],
    ['watch', 'Real-time monitoring, hooks, webhooks, and reconnect boundaries.'],
    ['Trust Model', 'What the CLI touches and how to reason about risk.'],
  ];

  return (
    <div className="mt-auto flex select-none flex-col rounded-xl border bg-fd-popover mask-[linear-gradient(to_bottom,white_40%,transparent_90%)] max-md:-mx-4">
      <div className="inline-flex items-center gap-2 px-4 py-3 text-sm text-fd-muted-foreground">
        <SearchIcon className="size-4" />
        Search...
      </div>
      <div className="border-t p-2">
        {items.map(([title, description], i) => (
          <div key={title} className={cn('rounded-md p-2 text-sm text-fd-popover-foreground', i === 0 && 'bg-fd-accent')}>
            <div className="flex flex-row items-center gap-2">
              <BookIcon className="size-4 text-fd-muted-foreground" />
              <p>{title}</p>
            </div>
            <p className="mt-2 ps-6 text-xs text-fd-muted-foreground">{description}</p>
          </div>
        ))}
      </div>
    </div>
  );
}

function OpenSource() {
  return (
    <>
      <h2 className={cn(headingVariants({ variant: 'h2', className: 'col-span-full mt-8 mb-4 text-center text-brand' }))}>
        Operate With Clear Boundaries.
      </h2>

      <div className={cn(cardVariants({ className: 'flex flex-col' }))}>
        <ShieldCheck className="mb-4 text-brand" />
        <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-6' }))}>Trust is part of the product.</h3>
        <p className="mb-8">
          OpenKakao is useful because it stays close to the real app. The docs treat trust, limitations,
          and unattended policy as first-class topics for the same reason.
        </p>
        <div className="mb-8 flex flex-row items-center gap-2">
          <Link href="/docs/security/trust-model" className={cn(buttonVariants({ variant: 'primary' }))}>
            Trust model
          </Link>
          <Link href="/docs/security/safe-usage" className={cn(buttonVariants({ variant: 'secondary' }))}>
            Safe usage
          </Link>
        </div>
      </div>

      <div className={cn(cardVariants({ className: 'flex flex-col p-0 pt-8' }))}>
        <h2 className="mb-4 text-center font-mono text-3xl font-extrabold uppercase lg:text-4xl">
          Build Your Workflow
        </h2>
        <p className="mb-8 text-center font-mono text-xs opacity-50">
          local, scriptable, and explicit about its boundaries.
        </p>
        <div className="mt-auto h-[200px] overflow-hidden bg-gradient-to-b from-brand-secondary/10 p-8">
          <div className="mx-auto size-[500px] rounded-full bg-radial-[circle_at_0%_100%] from-brand-secondary to-transparent from-60%" />
        </div>
      </div>

      <ul className={cn(cardVariants({ className: 'col-span-full flex flex-col gap-6' }))}>
        <li>
          <span className="flex flex-row items-center gap-2 font-medium">
            <BatteryChargingIcon className="size-5" />
            Local-first by default.
          </span>
          <span className="mt-2 text-sm text-fd-muted-foreground">
            The intended trust boundary is your machine to Kakao, not your machine to an OpenKakao backend.
          </span>
        </li>
        <li>
          <span className="flex flex-row items-center gap-2 font-medium">
            <BellRing className="size-5" />
            Event-driven when needed.
          </span>
          <span className="mt-2 text-sm text-fd-muted-foreground">
            Use watch mode when polling is no longer enough, but keep delivery guarantees and retries in your own wrapper.
          </span>
        </li>
        <li>
          <span className="flex flex-row items-center gap-2 font-medium">
            <Webhook className="size-5" />
            Narrow side effects.
          </span>
          <span className="mt-2 text-sm text-fd-muted-foreground">
            Hooks and webhooks are explicit surfaces, not hidden background behavior.
          </span>
        </li>
        <li>
          <span className="flex flex-row items-center gap-2 font-medium">
            <TimerIcon className="size-5" />
            Fast to start.
          </span>
          <span className="mt-2 text-sm text-fd-muted-foreground">
            Install, authenticate, and read the first chat in minutes, then deepen only the workflows you actually need.
          </span>
        </li>
        <li className="mt-auto flex flex-row flex-wrap gap-2">
          <Link href="/docs" className={cn(buttonVariants())}>
            Read docs
          </Link>
          <a
            href="https://github.com/JungHoonGhae/openkakao"
            rel="noreferrer noopener"
            className={cn(buttonVariants({ variant: 'secondary' }))}
          >
            Open GitHub
          </a>
        </li>
      </ul>
    </>
  );
}
