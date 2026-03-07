import { getPageImage, source, sourceKo } from '@/lib/source';
import { DocsBody, DocsDescription, DocsPage, DocsTitle } from 'fumadocs-ui/layouts/docs/page';
import { notFound } from 'next/navigation';
import { getMDXComponents } from '@/mdx-components';
import type { Metadata } from 'next';
import { createRelativeLink } from 'fumadocs-ui/mdx';
import { LLMCopyButton, ViewOptions } from '@/components/ai/page-actions';
import { gitConfig } from '@/lib/layout.shared';

export default async function Page(props: PageProps<'/ko/docs/[[...slug]]'>) {
  const params = await props.params;
  const koPage = sourceKo.getPage(params.slug);
  const page = koPage ?? source.getPage(params.slug);
  if (!page) notFound();

  const MDX = page.data.body;
  const basePath = process.env.NEXT_PUBLIC_BASE_PATH ?? '';
  const markdownUrl = `${basePath}/llms.mdx/docs/${[...page.slugs, 'index.mdx'].join('/')}`;
  const contentRoot = koPage ? 'website/content/docs-ko' : 'website/content/docs';

  return (
    <DocsPage toc={page.data.toc} full={page.data.full}>
      <DocsTitle>{page.data.title}</DocsTitle>
      <DocsDescription className="mb-0">{page.data.description}</DocsDescription>
      <div className="flex flex-row gap-2 items-center border-b pb-6">
        <LLMCopyButton markdownUrl={markdownUrl} />
        <ViewOptions
          markdownUrl={markdownUrl}
          githubUrl={`https://github.com/${gitConfig.user}/${gitConfig.repo}/blob/${gitConfig.branch}/${contentRoot}/${page.path}`}
        />
      </div>
      <DocsBody>
        <MDX
          components={getMDXComponents({
            a: createRelativeLink(koPage ? sourceKo : source, page),
          })}
        />
      </DocsBody>
    </DocsPage>
  );
}

export async function generateStaticParams() {
  const seen = new Set<string>();
  const params = [];

  for (const item of [...sourceKo.generateParams(), ...source.generateParams()]) {
    const key = item.slug.join('/');
    if (seen.has(key)) continue;
    seen.add(key);
    params.push(item);
  }

  return params;
}

export async function generateMetadata(props: PageProps<'/ko/docs/[[...slug]]'>): Promise<Metadata> {
  const params = await props.params;
  const page = sourceKo.getPage(params.slug) ?? source.getPage(params.slug);
  if (!page) notFound();

  return {
    title: page.data.title,
    description: page.data.description,
    openGraph: {
      images: getPageImage(page).url,
    },
  };
}
