import Link from 'next/link';

export default function HomePage() {
  return (
    <div className="flex flex-1 flex-col justify-center text-center">
      <h1 className="mb-4 text-2xl font-bold">OpenKakao</h1>
      <p className="mx-auto max-w-2xl text-fd-muted-foreground">
        Unofficial KakaoTalk CLI for macOS. Read chats, inspect message history, watch events,
        and build local workflows from the terminal.
      </p>
      <div className="mt-6 flex flex-wrap items-center justify-center gap-3">
        <Link href="/docs" className="font-medium underline underline-offset-4">
          Open docs
        </Link>
        <Link href="/docs/getting-started/quickstart" className="font-medium underline underline-offset-4">
          Quickstart
        </Link>
        <Link href="/docs/security/trust-model" className="font-medium underline underline-offset-4">
          Trust model
        </Link>
      </div>
    </div>
  );
}
