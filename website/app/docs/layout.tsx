import { source } from '@/lib/source';
import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import { baseOptions } from '@/lib/layout.shared';

export default function Layout({ children }: LayoutProps<'/docs'>) {
  return (
    <DocsLayout
      tree={source.getPageTree()}
      {...baseOptions()}
      sidebar={{
        banner: (
          <div className="rounded-2xl border border-amber-300/40 bg-amber-50 px-3 py-3 text-sm text-amber-950 dark:border-amber-200/10 dark:bg-amber-300/10 dark:text-amber-100">
            Start with <a className="font-semibold underline underline-offset-4" href="/docs/security/trust-model">Security</a> if this is your first time evaluating the project.
          </div>
        ),
      }}
    >
      {children}
    </DocsLayout>
  );
}
