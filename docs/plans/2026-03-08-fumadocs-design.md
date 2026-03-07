# Fumadocs Migration Design

## Goal
Adopt the official Fumadocs Next.js MDX template as a dedicated documentation app for OpenKakao.

## Decisions
- Place the docs app in `website/` to keep Rust and Node concerns isolated.
- Use the official `+next+fuma-docs-mdx+static` template shape because the repository already deploys docs to GitHub Pages.
- Move site-facing MDX content into `website/content/docs` and keep root `docs/` for planning/archive material.
- Preserve the current information architecture: introduction, installation, quickstart, CLI, guides, protocol.
- Add Mermaid support because existing docs already contain Mermaid code blocks.

## Scope
- Scaffold a Next.js + Fumadocs site in `website/`.
- Migrate existing MDX files and assets required by the site.
- Configure sidebar metadata, homepage, search route, and page rendering.
- Update internal doc links to `/docs/...` paths.
- Validate with install, typecheck, and production build.

## Risks
- Fumadocs route conventions differ from the previous plain MDX layout, so internal links must be normalized.
- Existing Mermaid blocks require explicit configuration.
- Some root `docs/` content is non-site material and should not be migrated as public docs.

## Validation
- `pnpm install`
- `pnpm types:check`
- `pnpm build`
