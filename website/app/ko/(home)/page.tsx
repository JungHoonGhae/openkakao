import Link from 'next/link';

const useCases = [
  {
    title: '읽지 않은 메시지 분류와 일일 요약',
    body: '직접 KakaoTalk를 반복 확인하는 대신 unread 채팅을 로컬 요약, 운영자 큐, 개인 대시보드로 바꿉니다.',
    href: '/ko/docs/automation/common-recipes',
    label: '레시피 보기',
  },
  {
    title: '채팅 export 파이프라인',
    body: '데스크톱 앱을 워크플로 도구처럼 억지로 쓰지 않고, 메시지 히스토리를 JSON, SQLite, 로컬 검색으로 가져옵니다.',
    href: '/ko/docs/cli/message',
    label: '읽기와 export 보기',
  },
  {
    title: '이벤트 기반 알림',
    body: '새 메시지가 오면 watch 모드로 로컬 스크립트, webhook, 검토 흐름을 트리거할 수 있습니다.',
    href: '/ko/docs/cli/watch',
    label: 'watch 모드 보기',
  },
  {
    title: 'LLM과 에이전트 워크플로',
    body: 'KakaoTalk를 요약기, 분류기, 운영자용 agent의 입력 채널로 붙이되 로컬 스택과 가깝게 유지합니다.',
    href: '/ko/docs/automation/llm-agent-workflows',
    label: '워크플로 읽기',
  },
];

const storyPoints = [
  'KakaoTalk는 이미 요청, 업데이트, 조율, 문맥이 모이는 곳입니다.',
  '하지만 개인 채팅 워크플로는 개발자에게 구조적으로 닫혀 있습니다.',
  'OpenKakao는 그 표면을 로컬에서 열어 메시지를 자신이 통제하는 도구로 옮기게 합니다.',
];

const workflowSteps = [
  '인증 요청 재구성에 필요한 로컬 KakaoTalk 앱 상태를 읽습니다.',
  '가벼운 계정 점검과 캐시 기반 읽기에는 REST를 사용합니다.',
  '실제 채팅 워크플로, watch 모드, 미디어 흐름, 전송에는 LOCO를 사용합니다.',
  '출력은 JSON으로 내보내 셸, 데이터베이스, 에이전트와 조합합니다.',
];

const trustCards = [
  {
    title: '로컬 우선 경계',
    body: 'OpenKakao는 로그인된 macOS 앱 상태를 바탕으로 Kakao 엔드포인트와 직접 통신합니다. 별도 중계 서비스가 아닙니다.',
    href: '/ko/docs/security/trust-model',
    label: '신뢰 모델',
  },
  {
    title: '명시적인 데이터 처리',
    body: '문서는 무엇을 로컬에서 읽고 저장하며, 자동화 스택이 언제 프라이버시 모델을 바꾸는지 분명히 적습니다.',
    href: '/ko/docs/security/data-and-credentials',
    label: '데이터와 자격 증명',
  },
  {
    title: '신중한 아웃바운드 자동화',
    body: '실제 앱과 가깝기 때문에 유용하지만, 바로 그 이유로 민감합니다. side effect는 항상 명시적으로 남겨 둡니다.',
    href: '/ko/docs/security/safe-usage',
    label: '안전한 사용',
  },
];

const docPaths = [
  {
    title: '활용 사례',
    body: '설치 전에 먼저 OpenKakao가 어디에서 실제로 유용한지 확인합니다.',
    href: '/ko/docs/automation/overview',
  },
  {
    title: '빠른 시작',
    body: '설치, 인증, 채팅 목록 확인, 짧은 읽기까지 가장 짧은 경로로 진입합니다.',
    href: '/ko/docs/getting-started/quickstart',
  },
  {
    title: 'CLI 레퍼런스',
    body: '사용 적합성을 확인한 뒤 실제 명령 표면으로 들어갑니다.',
    href: '/ko/docs/cli/overview',
  },
  {
    title: '프로토콜 노트',
    body: '더 깊은 기술적 이해가 필요할 때 REST와 LOCO 동작을 확인합니다.',
    href: '/ko/docs/protocol/overview',
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
            macOS KakaoTalk를 위한 로컬 개발자 워크플로
          </p>
          <div className="space-y-4">
            <h1 className="max-w-4xl font-serif text-4xl font-semibold tracking-tight text-balance text-zinc-950 md:text-6xl dark:text-zinc-50">
              KakaoTalk를 로컬 워크플로 스택 안으로 가져오세요.
            </h1>
            <p className="max-w-3xl text-base leading-8 text-zinc-700 md:text-lg dark:text-zinc-300">
              OpenKakao는 개발자와 자동화 중심 사용자에게 KakaoTalk 채팅을 읽고, 이벤트를 감시하고,
              히스토리를 export하고, 신중한 메시지 워크플로를 구성할 수 있는 스크립터블 표면을 제공합니다.
              먼저 활용 사례를 보고, side effect를 자동화하기 전에는 신뢰 경계를 이해하세요.
            </p>
          </div>
          <div className="flex flex-wrap gap-3">
            <Link
              href="/ko/docs/automation/overview"
              className="rounded-full bg-zinc-950 px-5 py-3 text-sm font-semibold text-white transition hover:bg-zinc-800 dark:bg-zinc-100 dark:text-zinc-950 dark:hover:bg-zinc-200"
            >
              활용 사례 보기
            </Link>
            <Link
              href="/ko/docs/getting-started/quickstart"
              className="rounded-full border border-zinc-300 bg-white px-5 py-3 text-sm font-semibold text-zinc-900 transition hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-50 dark:hover:bg-zinc-800"
            >
              빠른 시작
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
              <p className="mt-2 text-sm leading-6 text-zinc-200">터미널 친화적인 흐름에서 채팅과 히스토리를 확인합니다.</p>
            </div>
            <div className="rounded-2xl border border-white/10 bg-white/5 p-4">
              <p className="text-xs uppercase tracking-[0.2em] text-emerald-200">Watch</p>
              <p className="mt-2 text-sm leading-6 text-zinc-200">메시지 이벤트가 오면 로컬 스크립트나 webhook을 트리거합니다.</p>
            </div>
            <div className="rounded-2xl border border-white/10 bg-white/5 p-4">
              <p className="text-xs uppercase tracking-[0.2em] text-emerald-200">Compose</p>
              <p className="mt-2 text-sm leading-6 text-zinc-200">JSON을 셸, 데이터베이스, 대시보드, agent 도구로 연결합니다.</p>
            </div>
          </div>
        </div>
      </section>

      <section id="use-cases" className="space-y-6">
        <div className="max-w-3xl space-y-4">
          <p className="text-xs font-semibold uppercase tracking-[0.24em] text-emerald-700 dark:text-emerald-300">
            활용 사례
          </p>
          <h2 className="font-serif text-3xl font-semibold text-zinc-950 dark:text-zinc-50">
            단순한 CLI가 아니라 실제 워크플로를 위한 도구입니다.
          </h2>
          <p className="text-sm leading-7 text-zinc-700 dark:text-zinc-300">
            OpenKakao는 더 큰 로컬 시스템 안에 들어갈 때 가장 유용합니다. 읽고, export하고,
            분류하고, 알리고, 검토한 뒤, 전송은 마지막 단계에서만 붙이세요.
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
            KakaoTalk는 이미 일의 일부입니다. 개발자 워크플로 표면은 그렇지 않습니다.
          </h2>
          <p className="text-sm leading-7 text-zinc-700 dark:text-zinc-300">
            많은 기술 사용자에게 KakaoTalk는 요청, 업데이트, 조율, 문맥이 이미 모이는 곳입니다.
            하지만 개인 채팅 워크플로는 구조적으로 닫혀 있습니다. 히스토리를 읽고, 이벤트에 반응하고,
            메시지 문맥을 로컬 도구로 옮기려면 보통 수작업이나 깨지기 쉬운 우회가 필요합니다.
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
            로컬 스택과 조합되도록 설계했습니다.
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
            href="/ko/docs/getting-started/transport-boundary"
          >
            REST vs LOCO 읽기
          </Link>
        </article>
        <article className="rounded-[2rem] border border-zinc-200 bg-[linear-gradient(135deg,rgba(16,185,129,0.08),rgba(255,255,255,0.96))] p-8 shadow-sm dark:border-zinc-800 dark:bg-[linear-gradient(135deg,rgba(16,185,129,0.08),rgba(9,9,11,0.96))]">
          <p className="text-xs font-semibold uppercase tracking-[0.24em] text-emerald-700 dark:text-emerald-300">
            Trust Boundary
          </p>
          <h2 className="mt-3 text-2xl font-semibold text-zinc-950 dark:text-zinc-50">
            실제 앱과 가깝기 때문에 유용하고, 바로 그 이유로 민감합니다.
          </h2>
          <p className="mt-4 text-sm leading-7 text-zinc-700 dark:text-zinc-300">
            OpenKakao는 로컬 앱 상태, 저장된 자격 증명, live messaging session을 기반으로 동작합니다.
            그래서 실제 워크플로에 유용합니다. 동시에 경계를 분명히 유지해야 합니다. 이 프로젝트는
            hosted relay가 아니라 local-first 도구이며, 문서는 무엇을 읽고 저장하고 어떤 자동화가 좁게
            유지돼야 하는지 의도적으로 분명히 적습니다.
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
            의도에 맞는 순서로 문서를 읽으세요.
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
        <h2 className="mt-3 text-3xl font-semibold">자동화 전에 먼저 읽으세요.</h2>
        <p className="mt-4 max-w-3xl text-sm leading-7 text-zinc-300">
          가장 좋은 첫 실행은 작고 관찰 가능한 실행입니다. 설치하고, 인증하고, 채팅 목록을 보고,
          짧은 메시지 구간만 읽은 뒤에야 send나 watch가 자신의 워크플로에 필요한지 결정하세요.
        </p>
        <div className="mt-5 flex flex-wrap gap-3">
          <Link href="/ko/docs/automation/overview" className="rounded-full bg-white px-5 py-3 text-sm font-semibold text-zinc-950 transition hover:bg-zinc-200">
            활용 사례 보기
          </Link>
          <Link href="/ko/docs/getting-started/quickstart" className="rounded-full border border-white/15 px-5 py-3 text-sm font-semibold text-white transition hover:bg-white/10">
            빠른 시작 열기
          </Link>
        </div>
      </section>
    </main>
  );
}
