import defaultMdxComponents from 'fumadocs-ui/mdx';
import type { MDXComponents } from 'mdx/types';
import { Mermaid } from 'mdx-mermaid/Mermaid';

export function getMDXComponents(components?: MDXComponents): MDXComponents {
  return {
    Mermaid,
    mermaid: Mermaid,
    ...defaultMdxComponents,
    ...components,
  };
}
