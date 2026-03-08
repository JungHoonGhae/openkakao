import { sourceKo } from '@/lib/source';
import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import { baseOptions, docsFooter } from '@/lib/layout.shared';

export default function Layout({ children }: LayoutProps<'/ko/docs'>) {
  return (
    <DocsLayout
      tree={sourceKo.getPageTree()}
      {...baseOptions('ko', 'docs')}
      sidebar={{
        banner: (
          <div className="rounded-2xl border border-amber-300/40 bg-amber-50 px-3 py-3 text-sm text-amber-950 dark:border-amber-200/10 dark:bg-amber-300/10 dark:text-amber-100">
            프로젝트 적합성을 먼저 보려면 <a className="font-semibold underline underline-offset-4" href="/ko/docs/automation/overview">활용 사례</a>부터 읽고, 신뢰 경계가 궁금하면 <a className="font-semibold underline underline-offset-4" href="/ko/docs/security/trust-model">보안 문서</a>로 넘어가세요.
          </div>
        ),
        footer: docsFooter('ko'),
      }}
    >
      {children}
    </DocsLayout>
  );
}
