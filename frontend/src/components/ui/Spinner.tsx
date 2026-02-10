import { cn } from '@/utils/cn';

const sizeMap = {
  sm: 'w-4 h-4',
  md: 'w-6 h-6',
  lg: 'w-8 h-8',
  xl: 'w-12 h-12',
};

interface SpinnerProps {
  size?: keyof typeof sizeMap;
  class?: string;
}

export function Spinner(props: SpinnerProps) {
  return (
    <svg
      class={cn('animate-spin text-accent', sizeMap[props.size ?? 'md'], props.class)}
      fill="none"
      viewBox="0 0 24 24"
      role="status"
      aria-label="加载中"
    >
      <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
      <path
        class="opacity-75"
        fill="currentColor"
        d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
      />
    </svg>
  );
}
