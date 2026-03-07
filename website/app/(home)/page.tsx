import Link from 'next/link';

export default function HomePage() {
  return (
    <main className="mx-auto flex w-full max-w-6xl flex-1 flex-col gap-10 px-6 py-16 md:px-10 md:py-24">
      <section className="grid gap-10 lg:grid-cols-[1.2fr_0.8fr] lg:items-center">
        <div className="space-y-6">
          <p className="inline-flex rounded-full border border-black/10 bg-white/80 px-3 py-1 text-sm font-medium text-black/70 shadow-sm backdrop-blur dark:border-white/10 dark:bg-white/5 dark:text-white/70">
            Reverse-engineered KakaoTalk automation for macOS
          </p>
          <div className="space-y-4">
            <h1 className="max-w-3xl text-4xl font-semibold tracking-tight text-balance md:text-6xl">
              OpenKakao documentation, rebuilt on Fumadocs.
            </h1>
            <p className="max-w-2xl text-base leading-7 text-black/70 md:text-lg dark:text-white/70">
              Learn how the Rust CLI authenticates with KakaoTalk, reads chat history, sends
              messages over LOCO, and plugs into Unix-style automation.
            </p>
          </div>
          <div className="flex flex-wrap gap-3">
            <Link
              href="/docs"
              className="rounded-full bg-black px-5 py-3 text-sm font-semibold text-white transition hover:bg-black/85 dark:bg-white dark:text-black dark:hover:bg-white/85"
            >
              Open docs
            </Link>
            <Link
              href="/docs/quickstart"
              className="rounded-full border border-black/10 bg-white/80 px-5 py-3 text-sm font-semibold text-black transition hover:bg-white dark:border-white/10 dark:bg-white/5 dark:text-white dark:hover:bg-white/10"
            >
              Quickstart
            </Link>
          </div>
        </div>

        <div className="overflow-hidden rounded-3xl border border-black/10 bg-zinc-950 p-5 text-sm text-zinc-100 shadow-2xl shadow-amber-200/30 dark:border-white/10 dark:shadow-none">
          <div className="mb-4 flex items-center gap-2 text-xs text-zinc-400">
            <span className="h-2.5 w-2.5 rounded-full bg-rose-400" />
            <span className="h-2.5 w-2.5 rounded-full bg-amber-400" />
            <span className="h-2.5 w-2.5 rounded-full bg-emerald-400" />
          </div>
          <pre className="overflow-x-auto">
            <code>{`brew tap JungHoonGhae/openkakao
brew install openkakao-rs

openkakao-rs login --save
openkakao-rs loco-chats
openkakao-rs loco-read <chat_id> --all
openkakao-rs send <chat_id> "Hello from CLI!"`}</code>
          </pre>
        </div>
      </section>

      <section className="grid gap-4 md:grid-cols-3">
        <article className="rounded-3xl border border-black/10 bg-white/80 p-6 shadow-sm backdrop-blur dark:border-white/10 dark:bg-white/5">
          <h2 className="text-lg font-semibold">CLI Reference</h2>
          <p className="mt-2 text-sm leading-6 text-black/70 dark:text-white/70">
            Commands for auth, chats, messages, media, diagnostics, and profile access.
          </p>
          <Link className="mt-4 inline-flex text-sm font-semibold underline-offset-4 hover:underline" href="/docs/cli/overview">
            Browse commands
          </Link>
        </article>
        <article className="rounded-3xl border border-black/10 bg-white/80 p-6 shadow-sm backdrop-blur dark:border-white/10 dark:bg-white/5">
          <h2 className="text-lg font-semibold">Guides</h2>
          <p className="mt-2 text-sm leading-6 text-black/70 dark:text-white/70">
            Setup, token refresh, real-time watch mode, file transfer, and automation recipes.
          </p>
          <Link className="mt-4 inline-flex text-sm font-semibold underline-offset-4 hover:underline" href="/docs/guides/automation">
            Read guides
          </Link>
        </article>
        <article className="rounded-3xl border border-black/10 bg-white/80 p-6 shadow-sm backdrop-blur dark:border-white/10 dark:bg-white/5">
          <h2 className="text-lg font-semibold">Protocol Notes</h2>
          <p className="mt-2 text-sm leading-6 text-black/70 dark:text-white/70">
            Reverse-engineered notes on Booking, Checkin, LOGINLIST, packet format, and crypto.
          </p>
          <Link className="mt-4 inline-flex text-sm font-semibold underline-offset-4 hover:underline" href="/docs/protocol/overview">
            Inspect protocol
          </Link>
        </article>
      </section>
    </main>
  );
}
