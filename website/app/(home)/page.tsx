import Link from 'next/link';

const useCases = [
  {
    title: 'Unread triage and daily summaries',
    body: 'Turn unread chats into local summaries, operator queues, and personal dashboards instead of checking KakaoTalk by hand.',
    href: '/docs/automation/common-recipes',
    label: 'Explore recipes',
  },
  {
    title: 'Chat export pipelines',
    body: 'Pull history into JSON, SQLite, local search, or audit trails without pretending the desktop app is a workflow tool.',
    href: '/docs/cli/message',
    label: 'See read and export',
  },
  {
    title: 'Event-driven notifications',
    body: 'Use watch mode to trigger local scripts, webhooks, and review flows when new messages arrive.',
    href: '/docs/cli/watch',
    label: 'Inspect watch mode',
  },
  {
    title: 'LLM and agent workflows',
    body: 'Use KakaoTalk as an input channel for summarizers, classifiers, and operator-facing agents that stay close to your local stack.',
    href: '/docs/automation/llm-agent-workflows',
    label: 'Read workflows',
  },
];

const storyPoints = [
  'KakaoTalk is already where requests, updates, and coordination happen.',
  'But personal chat workflows remain structurally closed to developers.',
  'OpenKakao opens that surface locally so messages can move into tools you control.',
];

const workflowSteps = [
  'Read local KakaoTalk app state needed for authenticated requests.',
  'Use REST for lightweight account checks and cache-backed reads.',
  'Use LOCO for real chat workflows, watch mode, media flows, and sending.',
  'Emit JSON so the output composes cleanly with shells, databases, and agents.',
];

const trustCards = [
  {
    title: 'Local-first boundary',
    body: 'OpenKakao works from your logged-in macOS app state and talks to Kakao endpoints directly. It is not a hosted relay.',
    href: '/docs/security/trust-model',
    label: 'Trust model',
  },
  {
    title: 'Explicit data handling',
    body: 'The docs spell out what is read locally, what is stored, and when your automation stack changes the privacy model.',
    href: '/docs/security/data-and-credentials',
    label: 'Data and credentials',
  },
  {
    title: 'Careful outbound automation',
    body: 'The project is useful because it is close to the real app. It is sensitive for the same reason, so side effects stay explicit.',
    href: '/docs/security/safe-usage',
    label: 'Safe usage',
  },
];

const docPaths = [
  {
    title: 'Use Cases',
    body: 'See the workflows that make OpenKakao worth evaluating before you install anything.',
    href: '/docs/automation/overview',
  },
  {
    title: 'Quickstart',
    body: 'Install, authenticate, list chats, and read a small slice before deciding how far to go.',
    href: '/docs/getting-started/quickstart',
  },
  {
    title: 'CLI Reference',
    body: 'Move from examples into the actual command surface once you know the fit is real.',
    href: '/docs/cli/overview',
  },
  {
    title: 'Protocol Notes',
    body: 'Dive into REST and LOCO behavior when you need deeper technical grounding.',
    href: '/docs/protocol/overview',
  },
];

const quickPath = [
  'brew tap JungHoonGhae/openkakao',
  'brew install openkakao-rs',
  'openkakao-rs login --save',
  'openkakao-rs loco-chats',
  'openkakao-rs loco-read <chat_id> -n 20',
];

export default function HomePage() {
  return (
    <main className="mx-auto flex w-full max-w-7xl flex-1 flex-col gap-20 px-6 pb-20 pt-12 md:px-10 md:pb-24 md:pt-16">
      <section className="grid gap-10 lg:grid-cols-[1.05fr_0.95fr] lg:items-start">
        <div className="space-y-6">
          <p className="inline-flex rounded-full border border-emerald-300/60 bg-emerald-50 px-3 py-1 text-sm font-medium text-emerald-950 shadow-sm dark:border-emerald-200/15 dark:bg-emerald-300/10 dark:text-emerald-100">
            Local developer workflows for KakaoTalk on macOS
          </p>
          <div className="space-y-4">
            <h1 className="max-w-4xl font-serif text-4xl font-semibold tracking-tight text-balance text-zinc-950 md:text-6xl dark:text-zinc-50">
              Bring KakaoTalk into your local workflow stack.
            </h1>
            <p className="max-w-3xl text-base leading-8 text-zinc-700 md:text-lg dark:text-zinc-300">
              OpenKakao gives developers and automation-heavy users a scriptable way to read chats,
              watch events, export history, and build careful message workflows on top of KakaoTalk.
              Start with the use cases. Learn the trust boundary before you automate side effects.
            </p>
          </div>
          <div className="flex flex-wrap gap-3">
            <Link
              href="/docs/automation/overview"
              className="rounded-full bg-zinc-950 px-5 py-3 text-sm font-semibold text-white transition hover:bg-zinc-800 dark:bg-zinc-100 dark:text-zinc-950 dark:hover:bg-zinc-200"
            >
              See use cases
            </Link>
            <Link
              href="/docs/getting-started/quickstart"
              className="rounded-full border border-zinc-300 bg-white px-5 py-3 text-sm font-semibold text-zinc-900 transition hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-50 dark:hover:bg-zinc-800"
            >
              Quickstart
            </Link>
          </div>
        </div>

        <div className="overflow-hidden rounded-[2rem] border border-zinc-200 bg-[radial-gradient(circle_at_top_left,_rgba(16,185,129,0.16),_transparent_34%),linear-gradient(180deg,#18181b_0%,#09090b_100%)] p-5 text-sm text-zinc-100 shadow-2xl shadow-emerald-200/35 dark:border-zinc-800 dark:shadow-none">
          <div className="mb-4 flex items-center justify-between gap-2 text-xs uppercase tracking-[0.2em] text-zinc-400">
            <span>Workflow snapshot</span>
            <span>Local-first</span>
          </div>
          <pre className="overflow-x-auto rounded-2xl border border-white/10 bg-black/30 p-4 leading-7 text-zinc-100">
            <code>{quickPath.join('\n')}</code>
          </pre>
          <div className="mt-5 grid gap-3 md:grid-cols-3">
            <div className="rounded-2xl border border-white/10 bg-white/5 p-4">
              <p className="text-xs uppercase tracking-[0.2em] text-emerald-200">Read</p>
              <p className="mt-2 text-sm leading-6 text-zinc-200">Inspect chats and history from a terminal-native workflow.</p>
            </div>
            <div className="rounded-2xl border border-white/10 bg-white/5 p-4">
              <p className="text-xs uppercase tracking-[0.2em] text-emerald-200">Watch</p>
              <p className="mt-2 text-sm leading-6 text-zinc-200">Trigger local scripts or webhooks when message events arrive.</p>
            </div>
            <div className="rounded-2xl border border-white/10 bg-white/5 p-4">
              <p className="text-xs uppercase tracking-[0.2em] text-emerald-200">Compose</p>
              <p className="mt-2 text-sm leading-6 text-zinc-200">Feed JSON into shells, databases, dashboards, and agent tools.</p>
            </div>
          </div>
        </div>
      </section>

      <section id="use-cases" className="space-y-6">
        <div className="max-w-3xl space-y-4">
          <p className="text-xs font-semibold uppercase tracking-[0.24em] text-emerald-700 dark:text-emerald-300">
            Use Cases
          </p>
          <h2 className="font-serif text-3xl font-semibold text-zinc-950 dark:text-zinc-50">
            Real workflows, not just another CLI.
          </h2>
          <p className="text-sm leading-7 text-zinc-700 dark:text-zinc-300">
            OpenKakao is most useful when it becomes one small part of a larger local system. Read,
            export, classify, notify, review, and only then decide whether sending belongs in the loop.
          </p>
        </div>
        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
          {useCases.map((card) => (
            <article
              key={card.title}
              className="rounded-[1.75rem] border border-zinc-200 bg-white p-6 shadow-sm transition hover:-translate-y-0.5 hover:border-zinc-300 dark:border-zinc-800 dark:bg-zinc-950 dark:hover:border-zinc-700"
            >
              <h3 className="text-lg font-semibold text-zinc-950 dark:text-zinc-50">{card.title}</h3>
              <p className="mt-3 text-sm leading-7 text-zinc-700 dark:text-zinc-300">{card.body}</p>
              <Link
                className="mt-4 inline-flex text-sm font-semibold text-emerald-800 underline-offset-4 hover:underline dark:text-emerald-300"
                href={card.href}
              >
                {card.label}
              </Link>
            </article>
          ))}
        </div>
      </section>

      <section className="grid gap-6 lg:grid-cols-[0.9fr_1.1fr] lg:items-start">
        <div className="space-y-4">
          <p className="text-xs font-semibold uppercase tracking-[0.24em] text-emerald-700 dark:text-emerald-300">
            Why This Exists
          </p>
          <h2 className="font-serif text-3xl font-semibold text-zinc-950 dark:text-zinc-50">
            KakaoTalk is already part of work. Developer workflow surface is not.
          </h2>
          <p className="text-sm leading-7 text-zinc-700 dark:text-zinc-300">
            For many technical users, KakaoTalk is where requests, updates, coordination, and context
            already live. But personal chat workflows remain structurally closed. Reading history,
            reacting to events, or moving message context into local tools usually means manual work,
            brittle workarounds, or nothing at all.
          </p>
        </div>
        <div className="grid gap-4 md:grid-cols-3">
          {storyPoints.map((item) => (
            <article
              key={item}
              className="rounded-[1.75rem] border border-zinc-200 bg-white p-5 shadow-sm dark:border-zinc-800 dark:bg-zinc-950"
            >
              <p className="text-sm leading-7 text-zinc-700 dark:text-zinc-300">{item}</p>
            </article>
          ))}
        </div>
      </section>

      <section className="grid gap-6 lg:grid-cols-[1fr_1fr]">
        <article className="rounded-[2rem] border border-zinc-200 bg-white p-8 shadow-sm dark:border-zinc-800 dark:bg-zinc-950">
          <p className="text-xs font-semibold uppercase tracking-[0.24em] text-emerald-700 dark:text-emerald-300">
            How It Works
          </p>
          <h2 className="mt-3 text-2xl font-semibold text-zinc-950 dark:text-zinc-50">
            Built to compose with your local stack.
          </h2>
          <ol className="mt-4 space-y-3 text-sm leading-7 text-zinc-700 dark:text-zinc-300">
            {workflowSteps.map((step, index) => (
              <li key={step}>
                {index + 1}. {step}
              </li>
            ))}
          </ol>
          <Link
            className="mt-5 inline-flex text-sm font-semibold text-emerald-800 underline-offset-4 hover:underline dark:text-emerald-300"
            href="/docs/getting-started/transport-boundary"
          >
            Read REST vs LOCO
          </Link>
        </article>
        <article className="rounded-[2rem] border border-zinc-200 bg-[linear-gradient(135deg,rgba(16,185,129,0.08),rgba(255,255,255,0.96))] p-8 shadow-sm dark:border-zinc-800 dark:bg-[linear-gradient(135deg,rgba(16,185,129,0.08),rgba(9,9,11,0.96))]">
          <p className="text-xs font-semibold uppercase tracking-[0.24em] text-emerald-700 dark:text-emerald-300">
            Trust Boundary
          </p>
          <h2 className="mt-3 text-2xl font-semibold text-zinc-950 dark:text-zinc-50">
            Useful because it stays close to the real app. Sensitive for the same reason.
          </h2>
          <p className="mt-4 text-sm leading-7 text-zinc-700 dark:text-zinc-300">
            OpenKakao works from local app state, stored credentials, and live messaging sessions.
            That makes it useful for real workflows. It also means the boundary has to stay explicit.
            The project is local-first, not a hosted relay, and the docs are deliberate about what is
            read, what is stored, and which automations should stay narrow.
          </p>
          <div className="mt-5 grid gap-4 md:grid-cols-3">
            {trustCards.map((card) => (
              <Link
                key={card.title}
                href={card.href}
                className="rounded-[1.5rem] border border-zinc-200 bg-white p-5 transition hover:border-zinc-400 dark:border-zinc-700 dark:bg-zinc-950 dark:hover:border-zinc-500"
              >
                <h3 className="font-semibold text-zinc-950 dark:text-zinc-50">{card.title}</h3>
                <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">{card.body}</p>
                <p className="mt-3 text-sm font-semibold text-emerald-800 dark:text-emerald-300">{card.label}</p>
              </Link>
            ))}
          </div>
        </article>
      </section>

      <section className="space-y-6">
        <div className="max-w-3xl space-y-4">
          <p className="text-xs font-semibold uppercase tracking-[0.24em] text-emerald-700 dark:text-emerald-300">
            Docs Paths
          </p>
          <h2 className="font-serif text-3xl font-semibold text-zinc-950 dark:text-zinc-50">
            Learn in the order that matches your intent.
          </h2>
        </div>
        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
          {docPaths.map((card) => (
            <Link
              key={card.title}
              href={card.href}
              className="rounded-[1.75rem] border border-zinc-200 bg-white p-6 shadow-sm transition hover:-translate-y-0.5 hover:border-zinc-300 dark:border-zinc-800 dark:bg-zinc-950 dark:hover:border-zinc-700"
            >
              <h3 className="text-lg font-semibold text-zinc-950 dark:text-zinc-50">{card.title}</h3>
              <p className="mt-3 text-sm leading-7 text-zinc-700 dark:text-zinc-300">{card.body}</p>
            </Link>
          ))}
        </div>
      </section>

      <section className="rounded-[2rem] border border-zinc-200 bg-zinc-950 p-8 text-zinc-100 shadow-xl shadow-emerald-200/25 dark:border-zinc-800 dark:shadow-none">
        <p className="text-xs font-semibold uppercase tracking-[0.24em] text-emerald-300">Start Narrow</p>
        <h2 className="mt-3 text-3xl font-semibold">Read before you automate.</h2>
        <p className="mt-4 max-w-3xl text-sm leading-7 text-zinc-300">
          The best first run is a small, observable one. Install, authenticate, list chats, read a
          single slice, and only then decide whether send or watch mode belongs in your workflow.
        </p>
        <div className="mt-5 flex flex-wrap gap-3">
          <Link href="/docs/automation/overview" className="rounded-full bg-white px-5 py-3 text-sm font-semibold text-zinc-950 transition hover:bg-zinc-200">
            Browse use cases
          </Link>
          <Link href="/docs/getting-started/quickstart" className="rounded-full border border-white/15 px-5 py-3 text-sm font-semibold text-white transition hover:bg-white/10">
            Open quickstart
          </Link>
        </div>
      </section>
    </main>
  );
}
