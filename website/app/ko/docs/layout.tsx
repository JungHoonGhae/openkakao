import { sourceKo } from '@/lib/source';
import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import { baseOptions } from '@/lib/layout.shared';

export default function Layout({ children }: LayoutProps<'/ko/docs'>) {
  return (
    <DocsLayout
      tree={sourceKo.getPageTree()}
      {...baseOptions('ko')}
      sidebar={{
        banner: (
          <div className="rounded-2xl border border-amber-300/40 bg-amber-50 px-3 py-3 text-sm text-amber-950 dark:border-amber-200/10 dark:bg-amber-300/10 dark:text-amber-100">
            처음 검토하는 경우 <a className="font-semibold underline underline-offset-4" href="/ko/docs/security/trust-model">보안 문서</a>부터 읽으세요.
          </div>
        ),
      }}
    >
      {children}
    </DocsLayout>
  );
}
