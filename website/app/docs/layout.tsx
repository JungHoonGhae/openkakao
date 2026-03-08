import { source } from '@/lib/source';
import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import { baseOptions, docsFooter } from '@/lib/layout.shared';

export default function Layout({ children }: LayoutProps<'/docs'>) {
  return (
    <DocsLayout
      tree={source.getPageTree()}
      {...baseOptions('en', 'docs')}
      sidebar={{
        banner: (
          <div className="rounded-2xl border border-amber-300/40 bg-amber-50 px-3 py-3 text-sm text-amber-950 dark:border-amber-200/10 dark:bg-amber-300/10 dark:text-amber-100">
            Start with <a className="font-semibold underline underline-offset-4" href="/docs/automation/overview">Use Cases</a> if you are evaluating fit. Move to <a className="font-semibold underline underline-offset-4" href="/docs/security/trust-model">Security</a> when you need the trust boundary.
          </div>
        ),
        footer: docsFooter('en'),
      }}
    >
      {children}
    </DocsLayout>
  );
}
