import { createSignal, Show, onMount, onCleanup } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { ProgressBar } from '@/components/ui/Progress';
import { Spinner } from '@/components/ui/Spinner';
import { cn } from '@/utils/cn';
import { learningApi } from '@/api/learning';
import { recordsApi } from '@/api/records';
import { uiStore } from '@/stores/ui';
import type { Word } from '@/types/word';

export default function FlashcardPage() {
  const navigate = useNavigate();
  const [words, setWords] = createSignal<Word[]>([]);
  const [index, setIndex] = createSignal(0);
  const [flipped, setFlipped] = createSignal(false);
  const [loading, setLoading] = createSignal(true);
  const [known, setKnown] = createSignal(0);
  const [unknown, setUnknown] = createSignal(0);
  const [sessionId, setSessionId] = createSignal('');
  const [done, setDone] = createSignal(false);

  onMount(async () => {
    try {
      const session = await learningApi.createSession();
      setSessionId(session.sessionId);
      const study = await learningApi.getStudyWords();
      if (study.words.length === 0) {
        uiStore.toast.warning('暂无单词');
        setDone(true);
      }
      setWords(study.words);
    } catch (err: unknown) {
      uiStore.toast.error('加载失败', err instanceof Error ? err.message : '');
    } finally {
      setLoading(false);
    }
  });

  function handleKeyDown(e: KeyboardEvent) {
    if (done()) return;
    if (e.key === ' ' || e.key === 'Enter') { e.preventDefault(); setFlipped(!flipped()); }
    if (e.key === 'ArrowRight' || e.key === '1') markKnown();
    if (e.key === 'ArrowLeft' || e.key === '2') markUnknown();
  }

  onMount(() => {
    document.addEventListener('keydown', handleKeyDown);
    onCleanup(() => document.removeEventListener('keydown', handleKeyDown));
  });

  function advance(correct: boolean) {
    const w = words()[index()];
    if (w) {
      recordsApi.create({
        wordId: w.id,
        isCorrect: correct,
        responseTimeMs: 0,
        sessionId: sessionId() || undefined,
      }).catch(() => {});
    }
    if (index() + 1 >= words().length) {
      setDone(true);
      return;
    }
    setIndex((i) => i + 1);
    setFlipped(false);
  }

  function markKnown() { setKnown((n) => n + 1); advance(true); }
  function markUnknown() { setUnknown((n) => n + 1); advance(false); }

  const current = () => words()[index()];

  return (
    <div class="max-w-lg mx-auto space-y-6 animate-fade-in-up">
      <h1 class="text-2xl font-bold text-content">闪记模式</h1>

      <Show when={!loading()} fallback={<div class="flex justify-center py-20"><Spinner size="lg" /></div>}>
        <Show when={!done()} fallback={
          <Card variant="elevated" class="text-center py-12">
            <h2 class="text-xl font-bold text-content mb-4">完成!</h2>
            <div class="flex gap-8 justify-center mb-6">
              <div><p class="text-2xl font-bold text-success">{known()}</p><p class="text-xs text-content-secondary">认识</p></div>
              <div><p class="text-2xl font-bold text-error">{unknown()}</p><p class="text-xs text-content-secondary">不认识</p></div>
            </div>
            <Button onClick={() => navigate('/flashcard', { replace: true })}>再来一组</Button>
          </Card>
        }>
          <Show when={words().length > 0}>
            <ProgressBar value={index() + 1} max={words().length} showLabel size="sm" />

            {/* Flashcard */}
            <div class="perspective cursor-pointer" onClick={() => setFlipped(!flipped())}>
              <div class={cn(
                'relative w-full h-64 transition-transform duration-500 preserve-3d',
                flipped() && 'rotate-y-180',
              )}>
                {/* Front */}
                <div class="absolute inset-0 backface-hidden">
                  <Card variant="glass" class="h-full flex flex-col items-center justify-center">
                    <p class="text-3xl font-bold text-content">{current()?.text}</p>
                    <Show when={current()?.pronunciation}>
                      <p class="text-content-tertiary mt-2">{current()?.pronunciation}</p>
                    </Show>
                    <p class="text-xs text-content-tertiary mt-4">点击翻转 / 空格键</p>
                  </Card>
                </div>
                {/* Back */}
                <div class="absolute inset-0 backface-hidden rotate-y-180">
                  <Card variant="glass" class="h-full flex flex-col items-center justify-center">
                    <p class="text-2xl font-bold text-content">{current()?.meaning}</p>
                    <Show when={current()?.partOfSpeech}>
                      <p class="text-sm text-content-tertiary mt-2">{current()?.partOfSpeech}</p>
                    </Show>
                    <Show when={current()?.examples && current()!.examples.length > 0}>
                      <p class="text-sm text-content-secondary mt-3 italic">"{current()!.examples[0]}"</p>
                    </Show>
                  </Card>
                </div>
              </div>
            </div>

            {/* Actions */}
            <div class="flex gap-4">
              <Button onClick={markUnknown} variant="danger" fullWidth size="lg">
                <svg class="w-5 h-5 mr-1" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" /></svg>
                不认识 (2/←)
              </Button>
              <Button onClick={markKnown} variant="success" fullWidth size="lg">
                <svg class="w-5 h-5 mr-1" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" /></svg>
                认识 (1/→)
              </Button>
            </div>
          </Show>
        </Show>
      </Show>
    </div>
  );
}
