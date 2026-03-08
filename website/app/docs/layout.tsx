import { source } from '@/lib/source';
import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import { baseOptions, docsLinkItems, logo } from '@/components/layouts/shared';
import { getSection } from '@/lib/source/navigation';

export default function Layout({ children }: LayoutProps<'/docs'>) {
  const base = baseOptions();

  return (
    <DocsLayout
      {...base}
      tree={source.getPageTree()}
      links={docsLinkItems}
      nav={{
        ...base.nav,
        title: <span className="max-md:hidden">{logo}</span>,
      }}
      sidebar={{
        tabs: {
          transform(option, node) {
            const meta = source.getNodeMeta(node);
            if (!meta || !node.icon) return option;

            const color = `var(--${getSection(meta.path)}-color, var(--color-fd-foreground))`;

            return {
              ...option,
              icon: (
                <div
                  className="[&_svg]:size-full rounded-lg size-full text-(--tab-color) max-md:bg-(--tab-color)/10 max-md:border max-md:p-1.5"
                  style={
                    {
                      '--tab-color': color,
                    } as object
                  }
                >
                  {node.icon}
                </div>
              ),
            };
          },
        },
      }}
    >
      {children}
    </DocsLayout>
  );
}
