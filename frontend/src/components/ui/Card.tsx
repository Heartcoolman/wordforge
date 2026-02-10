import { type JSX, splitProps } from 'solid-js';
import { cn } from '@/utils/cn';

const variants = {
  elevated: 'bg-surface-elevated shadow-md',
  outlined: 'bg-surface border border-border',
  filled: 'bg-surface-secondary',
  glass: 'bg-surface-elevated/80 backdrop-blur-sm border border-border/50 shadow-lg',
} as const;

interface CardProps extends JSX.HTMLAttributes<HTMLDivElement> {
  variant?: keyof typeof variants;
  padding?: 'none' | 'sm' | 'md' | 'lg';
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

  return (
    <div
      {...rest}
      class={cn(
        'rounded-xl transition-all duration-200',
        variants[local.variant ?? 'elevated'],
        paddingMap[local.padding ?? 'md'],
        local.hover && 'hover:shadow-lg hover:-translate-y-0.5 cursor-pointer',
        local.class,
      )}
    >
      {local.children}
    </div>
  );
}
