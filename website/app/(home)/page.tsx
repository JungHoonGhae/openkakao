import Link from 'next/link';

const trustPrinciples = [
  {
    title: 'Local-first trust boundary',
    body: 'The CLI works from your logged-in macOS KakaoTalk app state and talks to Kakao endpoints directly. It is not a hosted relay.',
  },
  {
    title: 'Risk boundaries documented',
    body: 'The docs lead with what the tool reads, what it stores, and which automation patterns increase account risk.',
  },
  {
    title: 'Built for operator workflows',
    body: 'JSON output, watch mode, and CLI composition make it useful for developers, agents, and careful automation pipelines.',
  },
];

const automationCards = [
  {
    title: 'Unread and chat summaries',
    body: 'Export recent messages, unread counts, and chat metadata into jq, sqlite, or local dashboards.',
    href: '/docs/automation/common-recipes',
    label: 'See recipes',
  },
  {
    title: 'LLM and agent triage',
    body: 'Feed recent message slices into summarizers, classifiers, and routing logic without pretending outbound automation is free of risk.',
    href: '/docs/automation/llm-agent-workflows',
    label: 'Read workflows',
  },
  {
    title: 'Real-time watch loops',
    body: 'Use watch mode for event-driven notifications, logging, and review queues with reconnect behavior you can reason about.',
    href: '/docs/automation/watch-patterns',
    label: 'Inspect watch mode',
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
    <main className="mx-auto flex w-full max-w-7xl flex-1 flex-col gap-16 px-6 py-14 md:px-10 md:py-20">
      <section className="grid gap-10 lg:grid-cols-[1.15fr_0.85fr] lg:items-start">
        <div className="space-y-6">
          <p className="inline-flex rounded-full border border-amber-300/60 bg-amber-50 px-3 py-1 text-sm font-medium text-amber-900 shadow-sm dark:border-amber-200/15 dark:bg-amber-300/10 dark:text-amber-100">
            Unofficial KakaoTalk CLI for macOS, designed for careful automation
          </p>
          <div className="space-y-4">
            <h1 className="max-w-4xl font-serif text-4xl font-semibold tracking-tight text-balance text-zinc-950 md:text-6xl dark:text-zinc-50">
              Read chats. Build workflows. Start with the trust boundary.
            </h1>
            <p className="max-w-3xl text-base leading-8 text-zinc-700 md:text-lg dark:text-zinc-300">
              OpenKakao gives developers and terminal-native users controlled access to KakaoTalk chats,
              messaging, and LOCO-backed workflows. The first question is not what it can do. It is what
              it touches, where the risk is, and how to use it without lying to yourself about the tradeoffs.
            </p>
          </div>
          <div className="flex flex-wrap gap-3">
            <Link
              href="/docs/getting-started/quickstart"
              className="rounded-full bg-zinc-950 px-5 py-3 text-sm font-semibold text-white transition hover:bg-zinc-800 dark:bg-zinc-100 dark:text-zinc-950 dark:hover:bg-zinc-200"
            >
              Get started
            </Link>
            <Link
              href="/docs/security/trust-model"
              className="rounded-full border border-zinc-300 bg-white px-5 py-3 text-sm font-semibold text-zinc-900 transition hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-50 dark:hover:bg-zinc-800"
            >
              Read security model
            </Link>
          </div>
        </div>

        <div className="overflow-hidden rounded-[2rem] border border-zinc-200 bg-[radial-gradient(circle_at_top_left,_rgba(251,191,36,0.18),_transparent_34%),linear-gradient(180deg,#18181b_0%,#09090b_100%)] p-5 text-sm text-zinc-100 shadow-2xl shadow-amber-200/40 dark:border-zinc-800 dark:shadow-none">
          <div className="mb-4 flex items-center justify-between gap-2 text-xs text-zinc-400">
            <div className="flex items-center gap-2">
              <span className="h-2.5 w-2.5 rounded-full bg-rose-400" />
              <span className="h-2.5 w-2.5 rounded-full bg-amber-400" />
              <span className="h-2.5 w-2.5 rounded-full bg-emerald-400" />
            </div>
            <span>Fast path</span>
          </div>
          <pre className="overflow-x-auto rounded-2xl border border-white/10 bg-black/30 p-4 leading-7">
            <code>{quickPath.join('\n')}</code>
          </pre>
          <div className="mt-4 grid gap-3 text-xs text-zinc-300 md:grid-cols-3">
            <div className="rounded-2xl border border-white/10 bg-white/5 p-3">
              Reads local KakaoTalk app state
            </div>
            <div className="rounded-2xl border border-white/10 bg-white/5 p-3">
              Talks to REST and LOCO endpoints
            </div>
            <div className="rounded-2xl border border-white/10 bg-white/5 p-3">
              Exports JSON into shell pipelines
            </div>
          </div>
        </div>
      </section>

      <section className="grid gap-4 md:grid-cols-3">
        {trustPrinciples.map((item) => (
          <article
            key={item.title}
            className="rounded-[1.75rem] border border-zinc-200 bg-white p-6 shadow-sm dark:border-zinc-800 dark:bg-zinc-950"
          >
            <p className="text-xs font-semibold uppercase tracking-[0.2em] text-amber-700 dark:text-amber-300">
              Trust
            </p>
            <h2 className="mt-3 text-xl font-semibold text-zinc-950 dark:text-zinc-50">{item.title}</h2>
            <p className="mt-3 text-sm leading-7 text-zinc-700 dark:text-zinc-300">{item.body}</p>
          </article>
        ))}
      </section>

      <section className="grid gap-6 rounded-[2rem] border border-zinc-200 bg-[linear-gradient(135deg,rgba(251,191,36,0.08),rgba(255,255,255,0.95))] p-8 dark:border-zinc-800 dark:bg-[linear-gradient(135deg,rgba(251,191,36,0.08),rgba(9,9,11,0.96))] lg:grid-cols-[0.8fr_1.2fr]">
        <div>
          <p className="text-xs font-semibold uppercase tracking-[0.24em] text-amber-700 dark:text-amber-300">
            Security & Trust
          </p>
          <h2 className="mt-3 font-serif text-3xl font-semibold text-zinc-950 dark:text-zinc-50">
            Transparent about what it reads, stores, and risks.
          </h2>
          <p className="mt-4 max-w-xl text-sm leading-7 text-zinc-700 dark:text-zinc-300">
            These docs do not hide the uncomfortable part. OpenKakao touches sensitive local app state,
            stores credentials on disk when asked, and can create visible account behavior. That is why the
            recommended path starts with read-only workflows and clearly marked boundaries.
          </p>
        </div>
        <div className="grid gap-4 md:grid-cols-3">
          <Link href="/docs/security/trust-model" className="rounded-[1.5rem] border border-zinc-200 bg-white p-5 transition hover:border-zinc-400 dark:border-zinc-700 dark:bg-zinc-950 dark:hover:border-zinc-500">
            <h3 className="font-semibold text-zinc-950 dark:text-zinc-50">Trust model</h3>
            <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">The local-first trust boundary and the main risk categories.</p>
          </Link>
          <Link href="/docs/security/data-and-credentials" className="rounded-[1.5rem] border border-zinc-200 bg-white p-5 transition hover:border-zinc-400 dark:border-zinc-700 dark:bg-zinc-950 dark:hover:border-zinc-500">
            <h3 className="font-semibold text-zinc-950 dark:text-zinc-50">Data and credentials</h3>
            <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">What is read locally, what is stored on disk, and which endpoints are contacted.</p>
          </Link>
          <Link href="/docs/security/safe-usage" className="rounded-[1.5rem] border border-zinc-200 bg-white p-5 transition hover:border-zinc-400 dark:border-zinc-700 dark:bg-zinc-950 dark:hover:border-zinc-500">
            <h3 className="font-semibold text-zinc-950 dark:text-zinc-50">Safe usage</h3>
            <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">Practical rules for keeping automation narrow, local, and reviewable.</p>
          </Link>
        </div>
      </section>

      <section className="grid gap-6 lg:grid-cols-[0.9fr_1.1fr] lg:items-start">
        <div className="space-y-4">
          <p className="text-xs font-semibold uppercase tracking-[0.24em] text-amber-700 dark:text-amber-300">
            What you can automate
          </p>
          <h2 className="font-serif text-3xl font-semibold text-zinc-950 dark:text-zinc-50">
            Built for developers, CLI users, and operator-grade local tooling.
          </h2>
          <p className="max-w-xl text-sm leading-7 text-zinc-700 dark:text-zinc-300">
            OpenKakao is strongest when used as one composable command in a larger local workflow. Read, filter,
            summarize, archive, notify, or draft. Add outbound behavior last.
          </p>
        </div>
        <div className="grid gap-4 md:grid-cols-3">
          {automationCards.map((card) => (
            <article key={card.title} className="rounded-[1.75rem] border border-zinc-200 bg-white p-6 shadow-sm dark:border-zinc-800 dark:bg-zinc-950">
              <h3 className="text-lg font-semibold text-zinc-950 dark:text-zinc-50">{card.title}</h3>
              <p className="mt-3 text-sm leading-7 text-zinc-700 dark:text-zinc-300">{card.body}</p>
              <Link className="mt-4 inline-flex text-sm font-semibold text-amber-800 underline-offset-4 hover:underline dark:text-amber-300" href={card.href}>
                {card.label}
              </Link>
            </article>
          ))}
        </div>
      </section>

      <section className="grid gap-6 lg:grid-cols-[1fr_1fr]">
        <article className="rounded-[2rem] border border-zinc-200 bg-white p-8 shadow-sm dark:border-zinc-800 dark:bg-zinc-950">
          <p className="text-xs font-semibold uppercase tracking-[0.24em] text-amber-700 dark:text-amber-300">
            How it works
          </p>
          <h2 className="mt-3 text-2xl font-semibold text-zinc-950 dark:text-zinc-50">macOS KakaoTalk state to REST and LOCO workflows</h2>
          <ol className="mt-4 space-y-3 text-sm leading-7 text-zinc-700 dark:text-zinc-300">
            <li>1. Read the local app state needed to reconstruct authenticated requests.</li>
            <li>2. Use REST endpoints for account, friend, and cached-message operations.</li>
            <li>3. Use LOCO sessions for real-time reads, sends, and watch-based flows.</li>
            <li>4. Emit JSON so the output can feed shells, databases, or agent tools.</li>
          </ol>
          <Link className="mt-5 inline-flex text-sm font-semibold text-amber-800 underline-offset-4 hover:underline dark:text-amber-300" href="/docs/protocol/overview">
            Inspect the protocol notes
          </Link>
        </article>
        <article className="rounded-[2rem] border border-zinc-200 bg-zinc-950 p-8 text-zinc-100 shadow-xl shadow-amber-200/25 dark:border-zinc-800 dark:shadow-none">
          <p className="text-xs font-semibold uppercase tracking-[0.24em] text-amber-300">Fast path</p>
          <h2 className="mt-3 text-2xl font-semibold">Read before you automate.</h2>
          <p className="mt-4 text-sm leading-7 text-zinc-300">
            The best first run is a small, observable one. Install, authenticate, list chats, read a single slice,
            and only then decide whether send or watch mode belongs in your workflow.
          </p>
          <div className="mt-5 flex flex-wrap gap-3">
            <Link href="/docs/getting-started/installation" className="rounded-full bg-white px-5 py-3 text-sm font-semibold text-zinc-950 transition hover:bg-zinc-200">
              Installation
            </Link>
            <Link href="/docs/getting-started/quickstart" className="rounded-full border border-white/15 px-5 py-3 text-sm font-semibold text-white transition hover:bg-white/10">
              Quick start
            </Link>
            <Link href="/docs/cli/overview" className="rounded-full border border-white/15 px-5 py-3 text-sm font-semibold text-white transition hover:bg-white/10">
              CLI reference
            </Link>
          </div>
        </article>
      </section>
    </main>
  );
}
