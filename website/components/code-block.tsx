import { cn } from '@/lib/cn';

type CodeBlockProps = {
  code: string;
  lang?: string;
  className?: string;
};

export function CodeBlock({ code, lang = 'txt', className }: CodeBlockProps) {
  return (
    <pre
      className={cn(
        'overflow-x-auto rounded-xl border bg-fd-card p-4 text-sm text-fd-card-foreground',
        className,
      )}
    >
      <code data-language={lang}>{code}</code>
    </pre>
  );
}
