import Link from 'next/link';

const primitives = [
  {
    name: 'Read',
    body: '어떤 워크플로를 만들지 결정하기 전에 먼저 메시지 히스토리를 JSON으로 읽어옵니다.',
    href: '/ko/docs/cli/message',
  },
  {
    name: 'Watch',
    body: '반응 속도가 중요할 때 새 이벤트를 로컬 스크립트, 훅, 검토 큐로 연결합니다.',
    href: '/ko/docs/cli/watch',
  },
  {
    name: 'Export',
    body: '채팅 구간을 SQLite, 검색 인덱스, 노트, 내부 도구로 옮겨 자신만의 표면을 만듭니다.',
    href: '/ko/docs/automation/common-recipes',
  },
  {
    name: 'Classify',
    body: '복잡한 메시지 흐름을 긴급도, triage 목록, 운영자 화면으로 재구성합니다.',
    href: '/ko/docs/automation/llm-agent-workflows',
  },
  {
    name: 'Trigger',
    body: 'webhook과 로컬 커맨드를 다른 시스템으로 넘기는 좁고 명시적인 handoff로 사용합니다.',
    href: '/ko/docs/automation/watch-patterns',
  },
  {
    name: 'Send carefully',
    body: '읽기 경로, 검토 경로, 신뢰 경계를 분명히 한 뒤에만 outbound action을 붙입니다.',
    href: '/ko/docs/security/safe-usage',
  },
];

const systemNotes = [
  '많은 기술 사용자에게 KakaoTalk는 이미 실제 문맥이 모이는 장소입니다.',
  'OpenKakao는 하나의 정해진 솔루션보다 building block을 먼저 드러냅니다.',
  '가치는 hosted layer를 숨기는 데서가 아니라, 자신만의 로컬 스택을 조합하는 데서 나옵니다.',
];

const trustLinks = [
  {
    title: '신뢰 모델',
    body: '무엇을 읽는지, 경계가 어디에 있는지, 왜 local-first가 중요한지 설명합니다.',
    href: '/ko/docs/security/trust-model',
  },
  {
    title: '데이터와 자격 증명',
    body: '무엇을 저장하는지, macOS 앱에서 무엇을 재사용하는지, 언제 프라이버시 모델이 바뀌는지 정리합니다.',
    href: '/ko/docs/security/data-and-credentials',
  },
  {
    title: 'REST vs LOCO',
    body: '가벼운 점검만으로 충분한 경우와 실제 채팅 워크플로에 live path가 필요한 경우를 나눕니다.',
    href: '/ko/docs/getting-started/transport-boundary',
  },
];

const entryLinks = [
  {
    title: '자동화 개요',
    body: '특정 레시피에 들어가기 전에 패턴과 primitive부터 봅니다.',
    href: '/ko/docs/automation/overview',
  },
  {
    title: '빠른 시작',
    body: '설치, 인증, 채팅 목록 확인, 짧은 읽기까지 실제 앱 상태를 기준으로 시작합니다.',
    href: '/ko/docs/getting-started/quickstart',
  },
  {
    title: 'CLI 레퍼런스',
    body: '랜딩 서사에서 실제 명령 표면으로 바로 들어갑니다.',
    href: '/ko/docs/cli/overview',
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
            macOS KakaoTalk를 위한 로컬 워크플로 표면
          </div>

          <div className="space-y-5">
            <h1 className="max-w-4xl text-5xl font-semibold tracking-[-0.05em] text-zinc-950 text-balance md:text-7xl dark:text-zinc-50">
              KakaoTalk를 실제 개발자 워크플로에 연결하세요.
            </h1>
            <p className="max-w-3xl text-base leading-8 text-zinc-600 md:text-lg dark:text-zinc-300">
              KakaoTalk에는 이미 요청, 업데이트, 조율, 개인 문맥이 쌓여 있습니다. 부족한 것은 개발자가 다룰 수 있는 워크플로 표면입니다. OpenKakao는 읽기, 감시, 내보내기, 분류, 트리거 같은 로컬 기본 동작을 제공해서 하나의 정해진 활용 사례가 아니라 각자 필요한 흐름을 조합할 수 있게 합니다.
            </p>
          </div>

          <div className="flex flex-wrap items-center gap-3">
            <Link
              href="/ko/docs/automation/overview"
              className="inline-flex items-center rounded-full bg-zinc-950 px-5 py-3 text-sm font-semibold text-white transition hover:bg-zinc-800 dark:bg-zinc-100 dark:text-zinc-950 dark:hover:bg-zinc-200"
            >
              기본 동작 보기
            </Link>
            <Link
              href="/ko/docs/getting-started/quickstart"
              className="inline-flex items-center rounded-full border border-zinc-200 bg-white px-5 py-3 text-sm font-semibold text-zinc-900 transition hover:border-zinc-300 hover:bg-zinc-50 dark:border-zinc-800 dark:bg-zinc-950 dark:text-zinc-100 dark:hover:border-zinc-700 dark:hover:bg-zinc-900"
            >
              빠른 시작
            </Link>
          </div>

          <div className="grid gap-3 sm:grid-cols-3">
            <div className="rounded-2xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-950">
              <p className="text-[11px] font-semibold uppercase tracking-[0.2em] text-zinc-500 dark:text-zinc-400">Read path</p>
              <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">side effect보다 확인 경로를 먼저 둡니다.</p>
            </div>
            <div className="rounded-2xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-950">
              <p className="text-[11px] font-semibold uppercase tracking-[0.2em] text-zinc-500 dark:text-zinc-400">Composable</p>
              <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">JSON 출력이 셸, 데이터베이스, 에이전트와 자연스럽게 맞물립니다.</p>
            </div>
            <div className="rounded-2xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-950">
              <p className="text-[11px] font-semibold uppercase tracking-[0.2em] text-zinc-500 dark:text-zinc-400">Boundary-aware</p>
              <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">실제 앱과 가깝기 때문에 유용하고, 그만큼 민감합니다.</p>
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
                  <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">앱 상태와 메시지 히스토리를 자신이 신뢰하는 도구로 가져옵니다.</p>
                </div>
                <div className="rounded-[1.2rem] border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-950">
                  <p className="text-xs font-semibold uppercase tracking-[0.18em] text-zinc-500 dark:text-zinc-400">Bridge</p>
                  <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">KakaoTalk를 전체 시스템이 아니라 더 큰 로컬 워크플로의 한 입력으로 다룹니다.</p>
                </div>
                <div className="rounded-[1.2rem] border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-950">
                  <p className="text-xs font-semibold uppercase tracking-[0.18em] text-zinc-500 dark:text-zinc-400">Control</p>
                  <p className="mt-2 text-sm leading-6 text-zinc-700 dark:text-zinc-300">검토와 경계를 명시한 뒤에만 outbound action을 붙입니다.</p>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      <section className="space-y-7">
        <div className="max-w-3xl space-y-3">
          <p className="text-[11px] font-semibold uppercase tracking-[0.24em] text-zinc-500 dark:text-zinc-400">핵심 동작</p>
          <h2 className="text-3xl font-semibold tracking-[-0.04em] text-zinc-950 md:text-4xl dark:text-zinc-50">
            정해진 플레이북이 아니라 기본 동작에서 시작합니다.
          </h2>
          <p className="text-sm leading-7 text-zinc-600 dark:text-zinc-300">
            이 도구는 하나의 정해진 SaaS 워크플로가 아닙니다. 기술 사용자가 읽고, 필터링하고, 내보내고, 분류하고, 트리거하는 자신만의 루프를 만들 수 있는 CLI 표면입니다.
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
          <p className="text-[11px] font-semibold uppercase tracking-[0.24em] text-zinc-500 dark:text-zinc-400">왜 필요한가</p>
          <h2 className="text-3xl font-semibold tracking-[-0.04em] text-zinc-950 md:text-4xl dark:text-zinc-50">
            KakaoTalk에는 실제 업무 문맥이 있지만, 개발자 워크플로 표면은 여전히 부족합니다.
          </h2>
          <p className="text-sm leading-7 text-zinc-600 dark:text-zinc-300">
            많은 사람에게 KakaoTalk는 이미 일이 오가는 곳입니다. 문제는 중요성이 아니라 깔끔하고 로컬에서 다룰 수 있는 개발자용 building block의 부재입니다. 그게 없으면 메시지 워크플로는 결국 수작업, 깨지기 쉬운 GUI 습관, 임시 복붙 파이프라인으로 흘러갑니다.
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
          <p className="text-[11px] font-semibold uppercase tracking-[0.24em] text-zinc-500 dark:text-zinc-400">신뢰 경계</p>
          <h2 className="mt-3 text-3xl font-semibold tracking-[-0.04em] text-zinc-950 dark:text-zinc-50">
            실제로 유용할 만큼 가깝고, 운영 가능할 만큼 경계를 분명히 둡니다.
          </h2>
          <p className="mt-4 text-sm leading-7 text-zinc-600 dark:text-zinc-300">
            OpenKakao는 로컬 앱 상태, 저장된 자격 증명, live messaging session을 기반으로 동작합니다.
            그래서 실제 워크플로에 쓸 수 있습니다. 동시에 신뢰 경계는 암묵적으로 두면 안 됩니다. 이 프로젝트는
            local-first를 유지하고, 문서는 무엇을 읽고 저장하며 언제 outbound 동작을 좁게 유지해야 하는지 분명하게 설명합니다.
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
          <p className="text-[11px] font-semibold uppercase tracking-[0.24em] text-zinc-400">시작 지점</p>
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
