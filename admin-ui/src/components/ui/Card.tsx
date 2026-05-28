import { type JSX, splitProps } from 'solid-js';
import { cn } from '@/utils/cn';

const variants = {
  elevated: 'bg-surface-elevated shadow-elevation-1',
  outlined: 'bg-surface border border-border-hairline',
  filled: 'bg-surface-secondary',
  // glass: backdrop-blur 在旧版 Safari 兼容差，必要时可考虑 @supports (backdrop-filter: blur(8px)) 守门
  glass: 'bg-surface-elevated/80 backdrop-blur-md border border-border-hairline shadow-elevation-3',
  // interactive — 默认呈现等同 elevated，hover 时多层抬升（替代旧 hover prop 的语义）
  interactive: 'bg-surface-elevated shadow-elevation-1',
} as const;

interface CardProps extends JSX.HTMLAttributes<HTMLDivElement> {
  variant?: keyof typeof variants;
  padding?: 'none' | 'sm' | 'md' | 'lg';
  /** 触发 hover 抬升效果。`interactive` variant 自动启用，无需再传。 */
  hover?: boolean;
}

const paddingMap = {
  none: '',
  sm: 'p-3',
  md: 'p-5',
  lg: 'p-6',
};

export function Card(props: CardProps) {
  const [local, rest] = splitProps(props, ['variant', 'padding', 'hover', 'class', 'children']);
  const isInteractive = () => local.variant === 'interactive' || local.hover;

  return (
    <div
      {...rest}
      class={cn(
        'rounded-xl',
        variants[local.variant ?? 'elevated'],
        paddingMap[local.padding ?? 'md'],
        // 仅可交互卡片才挂 transition + hover 抬升，避免静态卡浪费合成层
        isInteractive() && 'transition-[transform,box-shadow] duration-base ease-out-expo hover:shadow-elevation-3 hover:-translate-y-0.5 cursor-pointer',
        local.class,
      )}
    >
      {local.children}
    </div>
  );
}
