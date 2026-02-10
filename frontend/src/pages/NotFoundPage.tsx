import { A } from '@solidjs/router';
import { Button } from '@/components/ui/Button';

export default function NotFoundPage() {
  return (
    <div class="min-h-[60vh] flex flex-col items-center justify-center text-center animate-fade-in-up">
      <p class="text-7xl font-bold text-accent mb-2">404</p>
      <h1 class="text-2xl font-bold text-content mb-2">页面不存在</h1>
      <p class="text-content-secondary mb-6">你访问的页面可能已被移除或地址有误</p>
      <A href="/"><Button>返回首页</Button></A>
    </div>
  );
}
