import { source } from '@/lib/source';
import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import { baseOptions, linkItems, logo } from '@/components/layouts/shared';

export default function Layout({ children }: LayoutProps<'/docs'>) {
  const base = baseOptions();

  return (
    <DocsLayout
      {...base}
      tree={source.getPageTree()}
      links={linkItems.filter((item) => item.type === 'icon')}
      nav={{
        ...base.nav,
        title: <span className="max-md:hidden">{logo}</span>,
      }}
    >
      {children}
    </DocsLayout>
  );
}
