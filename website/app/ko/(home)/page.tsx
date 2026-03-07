import Link from 'next/link';

const trustPrinciples = [
  {
    title: '로컬 우선 신뢰 경계',
    body: 'CLI는 macOS KakaoTalk 앱 상태를 읽고 Kakao 엔드포인트와 직접 통신합니다. 별도 중계 서버를 두지 않습니다.',
  },
  {
    title: '위험 경계를 먼저 공개',
    body: '문서는 도구가 무엇을 읽고 저장하는지, 어떤 자동화가 계정 리스크를 높이는지부터 설명합니다.',
  },
  {
    title: '운영자 중심 워크플로',
    body: 'JSON 출력, watch 모드, CLI 조합을 통해 개발자와 자동화 파이프라인에 맞는 도구로 설계했습니다.',
  },
];

const automationCards = [
  {
    title: '읽지 않은 메시지와 채팅 요약',
    body: '최근 메시지, unread 개수, 채팅 메타데이터를 jq, sqlite, 로컬 대시보드로 흘려보낼 수 있습니다.',
    href: '/ko/docs/automation/common-recipes',
    label: '레시피 보기',
  },
  {
    title: 'LLM 및 에이전트 분류',
    body: '최근 메시지를 요약기, 분류기, 라우팅 로직에 전달할 수 있지만, 아웃바운드 자동화 리스크는 숨기지 않습니다.',
    href: '/ko/docs/automation/llm-agent-workflows',
    label: '워크플로 읽기',
  },
  {
    title: '실시간 watch 루프',
    body: '실시간 알림, 로깅, 검토 큐를 위해 watch 모드를 사용하고, 재연결 동작을 예측 가능하게 유지합니다.',
    href: '/ko/docs/automation/watch-patterns',
    label: 'watch 확인',
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
            신중한 자동화를 위한 비공식 KakaoTalk CLI for macOS
          </p>
          <div className="space-y-4">
            <h1 className="max-w-4xl font-serif text-4xl font-semibold tracking-tight text-balance text-zinc-950 md:text-6xl dark:text-zinc-50">
              채팅을 읽고, 워크플로를 만들고, 먼저 신뢰 경계부터 확인하세요.
            </h1>
            <p className="max-w-3xl text-base leading-8 text-zinc-700 md:text-lg dark:text-zinc-300">
              OpenKakao는 개발자와 터미널 친화적 사용자에게 KakaoTalk 채팅, 메시징, LOCO 기반 워크플로를 제공합니다.
              중요한 질문은 기능이 아니라, 무엇을 건드리고 어디에 위험이 있으며 어떤 트레이드오프를 감수하는지입니다.
            </p>
          </div>
          <div className="flex flex-wrap gap-3">
            <Link
              href="/ko/docs/getting-started/quickstart"
              className="rounded-full bg-zinc-950 px-5 py-3 text-sm font-semibold text-white transition hover:bg-zinc-800 dark:bg-zinc-100 dark:text-zinc-950 dark:hover:bg-zinc-200"
            >
              빠르게 시작
            </Link>
            <Link
              href="/ko/docs/security/trust-model"
              className="rounded-full border border-zinc-300 bg-white px-5 py-3 text-sm font-semibold text-zinc-900 transition hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-50 dark:hover:bg-zinc-800"
            >
              보안 모델 보기
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
            <span>빠른 경로</span>
          </div>
          <pre className="overflow-x-auto rounded-2xl border border-white/10 bg-black/30 p-4 leading-7">
            <code>{quickPath.join('\n')}</code>
          </pre>
          <div className="mt-4 grid gap-3 text-xs text-zinc-300 md:grid-cols-3">
            <div className="rounded-2xl border border-white/10 bg-white/5 p-3">로컬 KakaoTalk 앱 상태 읽기</div>
            <div className="rounded-2xl border border-white/10 bg-white/5 p-3">REST와 LOCO 엔드포인트 직접 사용</div>
            <div className="rounded-2xl border border-white/10 bg-white/5 p-3">JSON을 쉘 파이프라인으로 전달</div>
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
          <p className="text-xs font-semibold uppercase tracking-[0.24em] text-amber-700 dark:text-amber-300">보안 & 신뢰</p>
          <h2 className="mt-3 font-serif text-3xl font-semibold text-zinc-950 dark:text-zinc-50">
            무엇을 읽고 저장하며 어디서 위험해지는지 숨기지 않습니다.
          </h2>
          <p className="mt-4 max-w-xl text-sm leading-7 text-zinc-700 dark:text-zinc-300">
            OpenKakao는 민감한 로컬 앱 상태를 다루고, 요청 시 자격 증명을 디스크에 저장하며, 눈에 보이는 계정 행동을 만들 수 있습니다.
            그래서 권장 경로는 항상 읽기 중심 워크플로부터 시작합니다.
          </p>
        </div>
        <div className="grid gap-4 md:grid-cols-3">
          <Link href="/ko/docs/security/trust-model" className="rounded-[1.5rem] border border-zinc-200 bg-white p-5 transition hover:border-zinc-400 dark:border-zinc-700 dark:bg-zinc-950 dark:hover:border-zinc-500">
            <h3 className="font-semibold text-zinc-950 dark:text-zinc-50">신뢰 모델</h3>
            <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">로컬 우선 신뢰 경계와 주요 리스크 범주.</p>
          </Link>
          <Link href="/ko/docs/security/data-and-credentials" className="rounded-[1.5rem] border border-zinc-200 bg-white p-5 transition hover:border-zinc-400 dark:border-zinc-700 dark:bg-zinc-950 dark:hover:border-zinc-500">
            <h3 className="font-semibold text-zinc-950 dark:text-zinc-50">데이터와 자격 증명</h3>
            <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">무엇을 로컬에서 읽고, 디스크에 저장하고, 어떤 엔드포인트와 통신하는지.</p>
          </Link>
          <Link href="/ko/docs/security/safe-usage" className="rounded-[1.5rem] border border-zinc-200 bg-white p-5 transition hover:border-zinc-400 dark:border-zinc-700 dark:bg-zinc-950 dark:hover:border-zinc-500">
            <h3 className="font-semibold text-zinc-950 dark:text-zinc-50">안전한 사용</h3>
            <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">자동화를 좁고 로컬이며 검토 가능한 상태로 유지하는 규칙.</p>
          </Link>
        </div>
      </section>

      <section className="grid gap-6 lg:grid-cols-[0.9fr_1.1fr] lg:items-start">
        <div className="space-y-4">
          <p className="text-xs font-semibold uppercase tracking-[0.24em] text-amber-700 dark:text-amber-300">자동화 가능 범위</p>
          <h2 className="font-serif text-3xl font-semibold text-zinc-950 dark:text-zinc-50">
            개발자와 CLI 사용자, 운영 중심 로컬 도구를 위한 구조입니다.
          </h2>
          <p className="max-w-xl text-sm leading-7 text-zinc-700 dark:text-zinc-300">
            OpenKakao는 더 큰 로컬 워크플로 안에 들어갈 때 가장 강합니다. 읽고, 필터링하고, 요약하고, 아카이브하고, 알리고, 초안까지 만든 뒤 발송은 마지막에 붙이십시오.
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
    </main>
  );
}
