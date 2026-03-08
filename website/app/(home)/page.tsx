import Link from 'next/link';

const primitives = [
  {
    name: 'Read',
    body: 'Pull message history into JSON before you decide what belongs in a workflow.',
    href: '/docs/cli/message',
  },
  {
    name: 'Watch',
    body: 'React to new events with local scripts, hooks, or review queues when timing matters.',
    href: '/docs/cli/watch',
  },
  {
    name: 'Export',
    body: 'Move chat slices into SQLite, search indexes, notes, or your own internal tooling.',
    href: '/docs/automation/common-recipes',
  },
  {
    name: 'Classify',
    body: 'Turn noisy message streams into urgency buckets, triage lists, and operator-facing views.',
    href: '/docs/automation/llm-agent-workflows',
  },
  {
    name: 'Trigger',
    body: 'Use webhooks and local commands as narrow, explicit handoff points to other systems.',
    href: '/docs/automation/watch-patterns',
  },
  {
    name: 'Send carefully',
    body: 'Add outbound actions only after the read path, review path, and trust boundary are clear.',
    href: '/docs/security/safe-usage',
  },
];

const systemNotes = [
  'KakaoTalk is where context already lives for many technical users.',
  'OpenKakao exposes building blocks instead of pretending one fixed workflow fits everyone.',
  'The value comes from composing your own local stack, not from hiding complexity behind a hosted layer.',
];

const trustLinks = [
  {
    title: 'Trust model',
    body: 'What the project reads, where the boundary sits, and why local-first matters.',
    href: '/docs/security/trust-model',
  },
  {
    title: 'Data and credentials',
    body: 'What is stored, what is reused from the macOS app, and when your privacy model changes.',
    href: '/docs/security/data-and-credentials',
  },
  {
    title: 'REST vs LOCO',
    body: 'When lightweight checks are enough and when real chat workflows need the live path.',
    href: '/docs/getting-started/transport-boundary',
  },
];

const entryLinks = [
  {
    title: 'Automation overview',
    body: 'Start with patterns and primitives before narrowing yourself to one workflow.',
    href: '/docs/automation/overview',
  },
  {
    title: 'Quickstart',
    body: 'Install, authenticate, list chats, and read a small slice from the real app state.',
    href: '/docs/getting-started/quickstart',
  },
  {
    title: 'CLI reference',
    body: 'Go from the landing story into the actual command surface.',
    href: '/docs/cli/overview',
  },
];

const previewSteps = [
  'Unread -> classify -> review queue',
  'watch -> webhook -> local tools',
  'loco-read -> JSON -> search or notes',
];

const commandSnippet = [
  'openkakao-rs unread --json',
  'openkakao-rs watch --chat-id <chat_id>',
  'openkakao-rs loco-read <chat_id> -n 50 --json',
];

export default function HomePage() {
  return (
    <main className="mx-auto flex w-full max-w-7xl flex-1 flex-col gap-24 px-6 pb-24 pt-10 md:px-10 md:pt-14">
      <section className="grid gap-12 lg:grid-cols-[minmax(0,1.05fr)_minmax(0,0.95fr)] lg:items-center">
        <div className="space-y-8">
          <div className="inline-flex items-center gap-2 rounded-full border border-zinc-200 bg-white px-3 py-1.5 text-[11px] font-medium uppercase tracking-[0.2em] text-zinc-600 shadow-sm dark:border-zinc-800 dark:bg-zinc-950 dark:text-zinc-300">
            <span className="inline-block h-2 w-2 rounded-full bg-emerald-500" />
            macOS local workflow surface for KakaoTalk
          </div>

          <div className="space-y-5">
            <h1 className="max-w-4xl text-5xl font-semibold tracking-[-0.05em] text-zinc-950 text-balance md:text-7xl dark:text-zinc-50">
              Open KakaoTalk to real developer workflows.
            </h1>
            <p className="max-w-3xl text-base leading-8 text-zinc-600 md:text-lg dark:text-zinc-300">
              KakaoTalk already holds requests, updates, coordination, and personal context. What it lacks
              is a usable workflow surface. OpenKakao gives you local primitives to read, watch, export,
              classify, and trigger your own tooling without pretending there is only one right use case.
            </p>
          </div>

          <div className="flex flex-wrap items-center gap-3">
            <Link
              href="/docs/automation/overview"
              className="inline-flex items-center rounded-full bg-zinc-950 px-5 py-3 text-sm font-semibold text-white transition hover:bg-zinc-800 dark:bg-zinc-100 dark:text-zinc-950 dark:hover:bg-zinc-200"
            >
              Explore primitives
            </Link>
            <Link
              href="/docs/getting-started/quickstart"
              className="inline-flex items-center rounded-full border border-zinc-200 bg-white px-5 py-3 text-sm font-semibold text-zinc-900 transition hover:border-zinc-300 hover:bg-zinc-50 dark:border-zinc-800 dark:bg-zinc-950 dark:text-zinc-100 dark:hover:border-zinc-700 dark:hover:bg-zinc-900"
            >
              Quickstart
            </Link>
          </div>

          <div className="grid gap-3 sm:grid-cols-3">
            <div className="rounded-2xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-950">
              <p className="text-[11px] font-semibold uppercase tracking-[0.2em] text-zinc-500 dark:text-zinc-400">Read path</p>
              <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">Start from inspection, not side effects.</p>
            </div>
            <div className="rounded-2xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-950">
              <p className="text-[11px] font-semibold uppercase tracking-[0.2em] text-zinc-500 dark:text-zinc-400">Composable</p>
              <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">JSON output that fits shells, databases, and agents.</p>
            </div>
            <div className="rounded-2xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-950">
              <p className="text-[11px] font-semibold uppercase tracking-[0.2em] text-zinc-500 dark:text-zinc-400">Boundary-aware</p>
              <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">Sensitive because it stays close to the real app.</p>
            </div>
          </div>
        </div>

        <div className="relative">
          <div className="absolute inset-x-10 top-8 h-40 rounded-full bg-zinc-200/70 blur-3xl dark:bg-zinc-800/40" />
          <div className="relative overflow-hidden rounded-[2rem] border border-zinc-200 bg-white p-4 shadow-[0_30px_80px_-32px_rgba(24,24,27,0.28)] dark:border-zinc-800 dark:bg-zinc-950 dark:shadow-none">
            <div className="rounded-[1.6rem] border border-zinc-200 bg-zinc-50 p-5 dark:border-zinc-800 dark:bg-zinc-900">
              <div className="flex flex-wrap items-center gap-2 text-[11px] font-medium uppercase tracking-[0.18em] text-zinc-500 dark:text-zinc-400">
                <span className="rounded-full border border-zinc-200 bg-white px-2.5 py-1 dark:border-zinc-700 dark:bg-zinc-950">Unread</span>
                <span className="rounded-full border border-zinc-200 bg-white px-2.5 py-1 dark:border-zinc-700 dark:bg-zinc-950">Watch</span>
                <span className="rounded-full border border-zinc-200 bg-white px-2.5 py-1 dark:border-zinc-700 dark:bg-zinc-950">Export</span>
                <span className="rounded-full border border-zinc-200 bg-white px-2.5 py-1 dark:border-zinc-700 dark:bg-zinc-950">Trigger</span>
              </div>

              <div className="mt-5 grid gap-4 lg:grid-cols-[0.9fr_1.1fr]">
                <div className="rounded-[1.35rem] border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-950">
                  <p className="text-xs font-semibold uppercase tracking-[0.2em] text-zinc-500 dark:text-zinc-400">Workflow shell</p>
                  <div className="mt-4 space-y-3">
                    {previewSteps.map((step) => (
                      <div
                        key={step}
                        className="rounded-xl border border-zinc-200 bg-zinc-50 px-3 py-2 text-sm text-zinc-700 dark:border-zinc-800 dark:bg-zinc-900 dark:text-zinc-300"
                      >
                        {step}
                      </div>
                    ))}
                  </div>
                </div>

                <div className="rounded-[1.35rem] border border-zinc-200 bg-zinc-950 p-4 text-zinc-100 dark:border-zinc-700">
                  <div className="flex items-center justify-between text-[11px] uppercase tracking-[0.18em] text-zinc-400">
                    <span>Command surface</span>
                    <span>Local-first</span>
                  </div>
                  <pre className="mt-4 overflow-x-auto text-sm leading-7 text-zinc-100">
                    <code>{commandSnippet.join('\n')}</code>
                  </pre>
                </div>
              </div>

              <div className="mt-4 grid gap-3 sm:grid-cols-3">
                <div className="rounded-[1.2rem] border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-950">
                  <p className="text-xs font-semibold uppercase tracking-[0.18em] text-zinc-500 dark:text-zinc-400">Primitive</p>
                  <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">Read app state and message history into tooling you already trust.</p>
                </div>
                <div className="rounded-[1.2rem] border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-950">
                  <p className="text-xs font-semibold uppercase tracking-[0.18em] text-zinc-500 dark:text-zinc-400">Bridge</p>
                  <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">Use KakaoTalk as one input to a broader local workflow, not the entire system.</p>
                </div>
                <div className="rounded-[1.2rem] border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-950">
                  <p className="text-xs font-semibold uppercase tracking-[0.18em] text-zinc-500 dark:text-zinc-400">Control</p>
                  <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">Add outbound actions only after review and boundary decisions are explicit.</p>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      <section className="space-y-7">
        <div className="max-w-3xl space-y-3">
          <p className="text-[11px] font-semibold uppercase tracking-[0.24em] text-zinc-500 dark:text-zinc-400">Capabilities</p>
          <h2 className="text-3xl font-semibold tracking-[-0.04em] text-zinc-950 md:text-4xl dark:text-zinc-50">
            Start from primitives, not a fixed playbook.
          </h2>
          <p className="text-sm leading-7 text-zinc-600 dark:text-zinc-300">
            This is not one opinionated SaaS workflow. It is a CLI surface that lets technical users build
            their own loops around reading, filtering, exporting, classifying, and triggering.
          </p>
        </div>

        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
          {primitives.map((primitive) => (
            <Link
              key={primitive.name}
              href={primitive.href}
              className="group rounded-[1.75rem] border border-zinc-200 bg-white p-6 transition hover:-translate-y-0.5 hover:border-zinc-300 hover:shadow-sm dark:border-zinc-800 dark:bg-zinc-950 dark:hover:border-zinc-700"
            >
              <div className="flex items-center justify-between gap-4">
                <h3 className="text-xl font-semibold tracking-[-0.03em] text-zinc-950 dark:text-zinc-50">{primitive.name}</h3>
                <span className="text-sm text-zinc-400 transition group-hover:text-zinc-700 dark:group-hover:text-zinc-200">↗</span>
              </div>
              <p className="mt-3 text-sm leading-7 text-zinc-600 dark:text-zinc-300">{primitive.body}</p>
            </Link>
          ))}
        </div>
      </section>

      <section className="grid gap-6 lg:grid-cols-[0.95fr_1.05fr] lg:items-start">
        <div className="space-y-4 rounded-[2rem] border border-zinc-200 bg-zinc-50 p-8 dark:border-zinc-800 dark:bg-zinc-900">
          <p className="text-[11px] font-semibold uppercase tracking-[0.24em] text-zinc-500 dark:text-zinc-400">Why this exists</p>
          <h2 className="text-3xl font-semibold tracking-[-0.04em] text-zinc-950 md:text-4xl dark:text-zinc-50">
            KakaoTalk holds real work context. Developer workflow surfaces are still missing.
          </h2>
          <p className="text-sm leading-7 text-zinc-600 dark:text-zinc-300">
            For many people, KakaoTalk is already where work gets coordinated. The gap is not relevance. The
            gap is access to clean, local, developer-grade building blocks. Without that, message workflows
            collapse into manual repetition, brittle GUI habits, or ad hoc copy-paste pipelines.
          </p>
        </div>

        <div className="grid gap-4 md:grid-cols-3">
          {systemNotes.map((item) => (
            <article
              key={item}
              className="rounded-[1.75rem] border border-zinc-200 bg-white p-6 dark:border-zinc-800 dark:bg-zinc-950"
            >
              <p className="text-sm leading-7 text-zinc-700 dark:text-zinc-300">{item}</p>
            </article>
          ))}
        </div>
      </section>

      <section className="grid gap-6 lg:grid-cols-[1.1fr_0.9fr]">
        <article className="rounded-[2rem] border border-zinc-200 bg-white p-8 shadow-sm dark:border-zinc-800 dark:bg-zinc-950">
          <p className="text-[11px] font-semibold uppercase tracking-[0.24em] text-zinc-500 dark:text-zinc-400">Trust boundary</p>
          <h2 className="mt-3 text-3xl font-semibold tracking-[-0.04em] text-zinc-950 dark:text-zinc-50">
            Close enough to be useful. Explicit enough to stay operationally sane.
          </h2>
          <p className="mt-4 text-sm leading-7 text-zinc-600 dark:text-zinc-300">
            OpenKakao works from local app state, stored credentials, and live messaging sessions. That is
            why it can support real workflows. It also means trust boundaries cannot stay implicit. The
            project stays local-first, and the docs are deliberate about what is read, what is stored, and
            when outbound behavior should remain narrow.
          </p>
          <div className="mt-6 grid gap-4 md:grid-cols-3">
            {trustLinks.map((link) => (
              <Link
                key={link.title}
                href={link.href}
                className="rounded-[1.5rem] border border-zinc-200 bg-zinc-50 p-5 transition hover:border-zinc-300 hover:bg-white dark:border-zinc-800 dark:bg-zinc-900 dark:hover:border-zinc-700 dark:hover:bg-zinc-950"
              >
                <h3 className="text-base font-semibold text-zinc-950 dark:text-zinc-50">{link.title}</h3>
                <p className="mt-2 text-sm leading-6 text-zinc-600 dark:text-zinc-300">{link.body}</p>
              </Link>
            ))}
          </div>
        </article>

        <article className="rounded-[2rem] border border-zinc-200 bg-zinc-950 p-8 text-zinc-100 dark:border-zinc-800">
          <p className="text-[11px] font-semibold uppercase tracking-[0.24em] text-zinc-400">Where to start</p>
          <div className="mt-4 space-y-3">
            {entryLinks.map((entry) => (
              <Link
                key={entry.title}
                href={entry.href}
                className="block rounded-[1.35rem] border border-white/10 bg-white/5 p-5 transition hover:border-white/20 hover:bg-white/10"
              >
                <h3 className="text-base font-semibold text-white">{entry.title}</h3>
                <p className="mt-2 text-sm leading-6 text-zinc-300">{entry.body}</p>
              </Link>
            ))}
          </div>
        </article>
      </section>
    </main>
  );
}
