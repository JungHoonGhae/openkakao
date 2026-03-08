'use client';

import {
  Fragment,
  type HTMLAttributes,
  type ReactNode,
  useEffect,
  useState,
} from 'react';
import Image from 'next/image';
import { ArrowRight } from 'lucide-react';
import { cva } from 'class-variance-authority';
import { cn } from '@/lib/cn';
import MainImg from './main.png';
import OpenAPIImg from './openapi.png';
import NotebookImg from './notebook.png';

const previewButtonVariants = cva('h-8 w-24 rounded-full text-sm font-medium transition-colors', {
  variants: {
    active: {
      true: 'text-fd-primary-foreground',
      false: 'text-fd-muted-foreground',
    },
  },
});

export function CreateWorkflowAnimation(props: HTMLAttributes<HTMLDivElement>) {
  const command = 'openkakao-rs login --save';
  const tickTime = 100;
  const timeCommandEnter = command.length;
  const timeCommandRun = timeCommandEnter + 3;
  const timeCommandEnd = timeCommandRun + 4;
  const timeEnd = timeCommandEnd + 1;
  const [tick, setTick] = useState(timeEnd);

  useEffect(() => {
    const timer = setInterval(() => {
      setTick((prev) => (prev >= timeEnd ? prev : prev + 1));
    }, tickTime);

    return () => clearInterval(timer);
  }, [timeEnd]);

  return (
    <div
      {...props}
      onMouseEnter={() => {
        if (tick >= timeEnd) setTick(0);
      }}
    >
      <pre className="min-h-[220px] font-mono text-sm">
        <code className="grid gap-1">
          <span>
            {command.substring(0, tick)}
            {tick < timeCommandEnter && <span className="inline-block h-3 w-1 animate-pulse bg-fd-foreground" />}
          </span>
          {tick > timeCommandRun && (
            <Fragment>
              <span className="text-fd-muted-foreground">Reading local KakaoTalk state...</span>
              {tick > timeCommandRun + 1 && <span className="text-fd-muted-foreground">Extracting reusable credentials...</span>}
              {tick > timeCommandRun + 2 && <span className="text-fd-muted-foreground">Validating account session...</span>}
              {tick > timeCommandRun + 3 && <span className="font-medium text-brand">Login saved successfully.</span>}
            </Fragment>
          )}
        </code>
      </pre>
    </div>
  );
}

export function PreviewImages(props: HTMLAttributes<HTMLDivElement>) {
  const [active, setActive] = useState(0);
  const previews = [
    { image: MainImg, name: 'Docs' },
    { image: NotebookImg, name: 'History' },
    { image: OpenAPIImg, name: 'Automation' },
  ];

  return (
    <div {...props} className={cn('relative grid', props.className)}>
      <div className="absolute bottom-0 left-1/2 z-2 flex -translate-x-1/2 flex-row rounded-full border bg-fd-card p-0.5 shadow-xl">
        <div
          role="none"
          className="absolute z-[-1] h-8 w-24 rounded-full bg-fd-primary transition-transform"
          style={{ transform: `translateX(calc(var(--spacing) * 24 * ${active}))` }}
        />
        {previews.map((item, i) => (
          <button
            key={item.name}
            className={cn(previewButtonVariants({ active: active === i }))}
            onClick={() => setActive(i)}
          >
            {item.name}
          </button>
        ))}
      </div>
      {previews.map((item, i) => (
        <Image
          key={item.name}
          src={item.image}
          alt={item.name}
          className={cn(
            'col-start-1 row-start-1 select-none rounded-2xl border shadow-lg',
            active === i ? 'animate-in slide-in-from-bottom-12 fade-in duration-800' : 'invisible',
          )}
        />
      ))}
    </div>
  );
}

const writingTabs = [
  { name: 'Operator', value: 'operator' },
  { name: 'Developer', value: 'developer' },
  { name: 'Automation', value: 'automation' },
] as const;

export function WorkflowTabs({
  tabs,
}: {
  tabs: Record<(typeof writingTabs)[number]['value'], ReactNode>;
}) {
  const [tab, setTab] = useState<(typeof writingTabs)[number]['value']>('operator');

  return (
    <div className="col-span-full my-20">
      <h2 className="mb-8 text-center text-4xl font-medium tracking-tight text-brand">
        One surface, many workflows.
      </h2>
      <p className="mx-auto mb-8 w-full max-w-[800px] text-center">
        OpenKakao is most useful when you use it as a narrow local bridge: inspect first, structure next,
        and automate only where the boundary is explicit.
      </p>
      <div className="mb-6 flex items-center justify-center gap-4 text-fd-muted-foreground">
        {writingTabs.map((item) => (
          <Fragment key={item.value}>
            <ArrowRight className="size-4 first:hidden" />
            <button
              className={cn('text-lg font-medium transition-colors', item.value === tab && 'text-brand')}
              onClick={() => setTab(item.value)}
            >
              {item.name}
            </button>
          </Fragment>
        ))}
      </div>
      {Object.entries(tabs).map(([key, value]) => (
        <div key={key} aria-hidden={key !== tab} className={cn('animate-fd-fade-in', key !== tab && 'hidden')}>
          {value}
        </div>
      ))}
    </div>
  );
}
