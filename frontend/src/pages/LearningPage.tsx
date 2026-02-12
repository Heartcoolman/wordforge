import { createSignal, Show, onMount, onCleanup, For } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Modal } from '@/components/ui/Modal';
import { Input } from '@/components/ui/Input';
import { ProgressBar } from '@/components/ui/Progress';
import { Spinner } from '@/components/ui/Spinner';
import { cn } from '@/utils/cn';
import { learningApi } from '@/api/learning';
import { recordsApi } from '@/api/records';
import { learningStore } from '@/stores/learning';
import { uiStore } from '@/stores/ui';
import { createWordQueueManager, type QueuedWord } from '@/lib/WordQueueManager';
import { fatigueStore } from '@/stores/fatigue';
import { useFatigueDetection } from '@/hooks/useFatigueDetection';
import { FatigueToggle } from '@/components/fatigue/FatigueToggle';
import { FatigueIndicator } from '@/components/fatigue/FatigueIndicator';
import { FatigueWarningModal } from '@/components/fatigue/FatigueWarningModal';
import { CameraPermission } from '@/components/fatigue/CameraPermission';
import { tokenManager } from '@/lib/token';
import type { CrossSessionHint } from '@/types/learning';
import {
  FATIGUE_WARNING_COOLDOWN_MS,
  MASTERY_COMPLETE_DELAY_MS,
  FEEDBACK_CORRECT_DELAY_MS,
  FEEDBACK_INCORRECT_DELAY_MS,
  GOAL_OPTIONS,
  MAX_CUSTOM_GOAL,
} from '@/lib/constants';
import { checkFatigueWarningCooldown } from '@/lib/fatigueWarningCooldown';

type Phase = 'goal-setup' | 'setup' | 'loading' | 'quiz' | 'feedback' | 'summary';

export default function LearningPage() {
  const navigate = useNavigate();
  const [phase, setPhase] = createSignal<Phase>('goal-setup');
  const [currentWord, setCurrentWord] = createSignal<QueuedWord | null>(null);
  const [options, setOptions] = createSignal<string[]>([]);
  const [selected, setSelected] = createSignal<string | null>(null);
  const [isCorrect, setIsCorrect] = createSignal(false);
  const [startTime, setStartTime] = createSignal(0);
  const [totalQuestions, setTotalQuestions] = createSignal(0);
  const [correctCount, setCorrectCount] = createSignal(0);
  const [targetMastery, setTargetMastery] = createSignal(10);
  const [sessionId, setSessionId] = createSignal('');
  const [crossSessionHint, setCrossSessionHint] = createSignal<CrossSessionHint | null>(null);
  const [prefetching, setPrefetching] = createSignal(false);

  // 自定义目标输入 Modal
  const [showGoalModal, setShowGoalModal] = createSignal(false);
  const [customGoalInput, setCustomGoalInput] = createSignal('');

  // 疲劳检测相关状态
  const [showCameraPermission, setShowCameraPermission] = createSignal(false);
  const [showFatigueWarning, setShowFatigueWarning] = createSignal(false);
  const { start: startDetection } = useFatigueDetection();

  const queue = createWordQueueManager(5);

  async function startWithGoal(goal: number) {
    setTargetMastery(goal);
    setPhase('loading');
    try {
      const session = await learningApi.createSession({ targetMasteryCount: goal });
      setSessionId(session.sessionId);
      setTargetMastery(session.targetMasteryCount || goal);
      learningStore.startSession(session.sessionId);

      if (session.crossSessionHint) {
        setCrossSessionHint(session.crossSessionHint);
      }

      if (!session.resumed) {
        queue.reset();
      }
      queue.setTargetMasteryCount(session.targetMasteryCount || goal);

      const study = await learningApi.getStudyWords();
      if (study.strategy?.batchSize) queue.setBatchSize(study.strategy.batchSize);

      if (study.words.length === 0) {
        uiStore.toast.warning('暂无可学习的单词', '请先添加单词或选择词书');
        setPhase('setup');
        return;
      }

      queue.loadWords(study.words);
      showNextQuestion();
    } catch (err: unknown) {
      uiStore.toast.error('加载失败', err instanceof Error ? err.message : '');
      setPhase('setup');
    }
  }

  // Keyboard shortcuts
  function handleKeyDown(e: KeyboardEvent) {
    if (phase() !== 'quiz') return;
    const opts = options();
    const key = parseInt(e.key);
    if (key >= 1 && key <= 4 && opts[key - 1]) {
      handleAnswer(opts[key - 1]);
    }
  }

  onMount(() => {
    setPhase('goal-setup');
    document.addEventListener('keydown', handleKeyDown);
  });

  onCleanup(() => {
    document.removeEventListener('keydown', handleKeyDown);
    // 使用 sendBeacon 同步进度，确保页面离开时仍能发送
    if (sessionId()) {
      const payload = JSON.stringify({
        sessionId: sessionId(),
        totalQuestions: totalQuestions(),
      });
      const token = tokenManager.getToken();
      const apiBase = (import.meta.env.VITE_API_BASE_URL as string | undefined)?.trim() || '';
      const url = `${apiBase}/api/learning/sync-progress`;
      const blob = new Blob([payload], { type: 'application/json' });
      // sendBeacon 无法设置 Authorization header，回退到 fetch + keepalive
      if (token) {
        fetch(url, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${token}` },
          body: payload,
          keepalive: true,
        }).catch(() => {});
      } else {
        navigator.sendBeacon(url, blob);
      }
    }
  });

  function showNextQuestion() {
    const next = queue.pickNext();
    if (!next) {
      // Check if we need more words
      if (queue.needsMoreWords()) {
        fetchMoreWords();
      } else {
        setPhase('summary');
      }
      return;
    }

    setCurrentWord(next);
    const mode = learningStore.mode();
    setOptions(queue.generateOptions(next, mode));
    setSelected(null);
    setIsCorrect(false);
    setStartTime(Date.now());
    setPhase('quiz');
  }

  async function fetchMoreWords() {
    try {
      const metrics = queue.computeSessionMetrics();
      const res = await learningApi.getNextWords({
        excludeWordIds: queue.getAllWordIds(),
        masteredWordIds: queue.getMasteredWordIds(),
        sessionId: sessionId() || undefined,
        sessionPerformance: totalQuestions() > 0 ? {
          recentAccuracy: metrics.recentAccuracy,
          overallAccuracy: metrics.overallAccuracy,
          masteredCount: queue.getMasteredCount(),
          targetMasteryCount: targetMastery(),
          errorProneWordIds: queue.getErrorProneWordIds(),
        } : undefined,
      });
      if (res.words.length === 0) {
        setPhase('summary');
        return;
      }
      queue.addWords(res.words);
      showNextQuestion();
    } catch {
      setPhase('summary');
    }
  }

  async function prefetchNextBatch() {
    if (prefetching()) return;
    setPrefetching(true);
    try {
      const metrics = queue.computeSessionMetrics();
      const res = await learningApi.getNextWords({
        excludeWordIds: queue.getAllWordIds(),
        masteredWordIds: queue.getMasteredWordIds(),
        sessionId: sessionId() || undefined,
        sessionPerformance: {
          recentAccuracy: metrics.recentAccuracy,
          overallAccuracy: metrics.overallAccuracy,
          masteredCount: queue.getMasteredCount(),
          targetMasteryCount: targetMastery(),
          errorProneWordIds: queue.getErrorProneWordIds(),
        },
      });
      if (res.words.length > 0) {
        queue.addWords(res.words);
      }
    } catch {
      // 预取失败不影响当前学习
    } finally {
      setPrefetching(false);
    }
  }

  async function completeAndShowSummary() {
    try {
      const metrics = queue.computeSessionMetrics();
      await learningApi.completeSession({
        sessionId: sessionId(),
        masteredWordIds: queue.getMasteredWordIds(),
        errorProneWordIds: queue.getErrorProneWordIds(),
        avgResponseTimeMs: Math.round(metrics.overallAvgResponseTimeMs),
      });
    } catch {
      // 完成请求失败不阻止显示总结
    }
    setPhase('summary');
  }

  async function handleAnswer(answer: string) {
    if (selected()) return; // prevent double click
    const cw = currentWord();
    if (!cw) return;

    const mode = learningStore.mode();
    const correctAnswer = mode === 'word-to-meaning' ? cw.word.meaning : cw.word.text;
    const correct = answer === correctAnswer;

    setSelected(answer);
    setIsCorrect(correct);
    setPhase('feedback');
    setTotalQuestions((n) => n + 1);
    if (correct) setCorrectCount((n) => n + 1);

    const responseTime = Date.now() - startTime();
    const result = queue.recordAnswer(cw.word.id, correct, responseTime);

    // Submit record to backend (async, don't block UI)
    recordsApi.create({
      clientRecordId: crypto.randomUUID(),
      wordId: cw.word.id,
      isCorrect: correct,
      responseTimeMs: responseTime,
      sessionId: sessionId() || undefined,
    }).catch((err: unknown) => {
      uiStore.toast.warning('答题记录保存失败', err instanceof Error ? err.message : '');
    });

    // 智能预取检查
    if (queue.shouldPrefetch() && !prefetching()) {
      prefetchNextBatch();
    }

    // Check completion
    if (result.mastered && queue.masteredCount() >= targetMastery()) {
      setTimeout(() => completeAndShowSummary(), MASTERY_COMPLETE_DELAY_MS);
      return;
    }

    // Auto advance after feedback
    setTimeout(() => {
      // 疲劳检测：答题后检查是否重度疲劳
      if (fatigueStore.detecting() && fatigueStore.fatigueLevel() === 'severe') {
        if (checkFatigueWarningCooldown()) {
          setShowFatigueWarning(true);
        }
      }

      if (queue.needsMoreWords() && queue.activeCount() === 0) {
        fetchMoreWords();
      } else {
        showNextQuestion();
      }
    }, correct ? FEEDBACK_CORRECT_DELAY_MS : FEEDBACK_INCORRECT_DELAY_MS);
  }

  function restartSession() {
    queue.reset();
    learningStore.clearSession();
    setTotalQuestions(0);
    setCorrectCount(0);
    setSessionId('');
    setCrossSessionHint(null);
    setPhase('goal-setup');
  }

  return (
    <div class="max-w-2xl mx-auto space-y-6 animate-fade-in-up">
      {/* Header */}
      <div class="flex items-center justify-between">
        <h1 class="text-2xl font-bold text-content">单词学习</h1>
        <div class="flex items-center gap-2">
          <Show when={fatigueStore.detecting()}>
            <FatigueIndicator />
          </Show>
          <FatigueToggle onFirstEnable={() => setShowCameraPermission(true)} />
          <button
            onClick={() => learningStore.toggleMode()}
            class="text-xs px-3 py-1.5 rounded-full bg-surface-tertiary text-content-secondary hover:text-content transition-colors cursor-pointer"
          >
            {learningStore.mode() === 'word-to-meaning' ? '英 → 中' : '中 → 英'}
          </button>
        </div>
      </div>

      {/* Progress — uses reactive signals */}
      <Show when={phase() !== 'setup' && phase() !== 'loading' && phase() !== 'goal-setup'}>
        <div class="space-y-1">
          <div class="flex justify-between text-xs text-content-secondary">
            <span>已掌握 {queue.masteredCount()}/{targetMastery()}</span>
            <span>第 {totalQuestions()} 题 | 正确率 {totalQuestions() > 0 ? Math.round((correctCount() / totalQuestions()) * 100) : 0}%</span>
          </div>
          <ProgressBar value={queue.masteredCount()} max={targetMastery()} color="success" />
        </div>
      </Show>

      {/* Goal Setup */}
      <Show when={phase() === 'goal-setup'}>
        <Card variant="elevated" class="py-8">
          <h2 class="text-xl font-semibold text-content text-center mb-6">设定学习目标</h2>

          {/* 跨会话提示 */}
          <Show when={crossSessionHint()}>
            {(hint) => (
              <div class="mx-6 mb-6 p-4 rounded-lg bg-accent/10 border border-accent/20">
                <p class="text-sm text-content-secondary">
                  上次学习：正确率 <span class="font-semibold text-content">{Math.round(hint().prevAccuracy * 100)}%</span>
                  ，掌握 <span class="font-semibold text-content">{hint().prevMasteredCount}</span> 个单词
                  （{hint().gapMinutes < 60 ? `${hint().gapMinutes} 分钟前` : `${Math.round(hint().gapMinutes / 60)} 小时前`}）
                </p>
              </div>
            )}
          </Show>

          <div class="grid grid-cols-2 sm:grid-cols-4 gap-3 px-6">
            <For each={[...GOAL_OPTIONS]}>
              {(goal) => (
                <button
                  onClick={() => startWithGoal(goal)}
                  class="p-4 rounded-xl border-2 border-border hover:border-accent bg-surface-elevated hover:shadow-md transition-all cursor-pointer text-center"
                >
                  <p class="text-2xl font-bold text-accent">{goal}</p>
                  <p class="text-xs text-content-secondary mt-1">个单词</p>
                </button>
              )}
            </For>
          </div>

          <div class="text-center mt-4">
            <button
              onClick={() => {
                setCustomGoalInput('');
                setShowGoalModal(true);
              }}
              class="text-sm text-content-tertiary hover:text-accent transition-colors cursor-pointer"
            >
              自定义数量...
            </button>
          </div>
        </Card>

        {/* 自定义目标数量 Modal */}
        <Modal open={showGoalModal()} onClose={() => setShowGoalModal(false)} title="自定义学习目标" size="sm">
          <div class="space-y-3 mt-2">
            <Input
              label="目标数量"
              type="number"
              placeholder="1-100"
              value={customGoalInput()}
              onInput={(e) => setCustomGoalInput(e.currentTarget.value)}
            />
            <div class="flex justify-end gap-2 pt-2">
              <Button variant="ghost" onClick={() => setShowGoalModal(false)}>取消</Button>
              <Button onClick={() => {
                const n = parseInt(customGoalInput());
                if (n > 0 && n <= MAX_CUSTOM_GOAL) {
                  setShowGoalModal(false);
                  startWithGoal(n);
                } else {
                  uiStore.toast.warning(`请输入 1-${MAX_CUSTOM_GOAL} 之间的数字`);
                }
              }}>确定</Button>
            </div>
          </div>
        </Modal>
      </Show>

      {/* Loading */}
      <Show when={phase() === 'loading'}>
        <div class="flex flex-col items-center justify-center py-20">
          <Spinner size="lg" />
          <p class="mt-4 text-content-secondary">正在准备学习内容...</p>
        </div>
      </Show>

      {/* Setup (no words) */}
      <Show when={phase() === 'setup'}>
        <Card variant="elevated" class="text-center py-12">
          <svg class="w-16 h-16 mx-auto text-content-tertiary mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" />
          </svg>
          <h2 class="text-xl font-semibold text-content mb-2">准备开始学习</h2>
          <p class="text-content-secondary mb-6">请先前往词库添加单词，或在设置中选择词书</p>
          <div class="flex gap-3 justify-center">
            <Button onClick={() => navigate('/vocabulary')} variant="outline">管理词库</Button>
            <Button onClick={() => navigate('/wordbooks')}>选择词书</Button>
          </div>
        </Card>
      </Show>

      {/* Quiz / Feedback */}
      <Show when={phase() === 'quiz' || phase() === 'feedback'}>
        <Show when={currentWord()}>
          {(cw) => {
            const correctAnswer = () => {
              const mode = learningStore.mode();
              return mode === 'word-to-meaning' ? cw().word.meaning : cw().word.text;
            };

            return (
              <div class="space-y-6">
                {/* Word Card */}
                <Card variant="glass" class="text-center py-10">
                  <Show when={learningStore.mode() === 'word-to-meaning'} fallback={
                    <div>
                      <p class="text-lg text-content-secondary mb-2">选择对应的单词</p>
                      <p class="text-2xl font-bold text-content">{cw().word.meaning}</p>
                    </div>
                  }>
                    <div>
                      <p class="text-3xl font-bold text-content mb-2">{cw().word.text}</p>
                      <Show when={cw().word.pronunciation}>
                        <p class="text-content-tertiary">{cw().word.pronunciation}</p>
                      </Show>
                    </div>
                  </Show>
                </Card>

                {/* Options */}
                <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                  <For each={options()}>
                    {(opt, i) => {
                      const isSelected = () => selected() === opt;
                      const isAnswer = () => opt === correctAnswer();
                      const showFeedback = () => phase() === 'feedback';

                      return (
                        <button
                          onClick={() => handleAnswer(opt)}
                          disabled={phase() === 'feedback'}
                          aria-label={`选项 ${i() + 1}: ${opt}`}
                          class={cn(
                            'relative p-4 rounded-xl border-2 text-left transition-all duration-200 cursor-pointer',
                            'hover:shadow-md active:scale-[0.98]',
                            !showFeedback() && 'border-border hover:border-accent bg-surface-elevated',
                            showFeedback() && isAnswer() && 'border-success bg-success-light animate-pulse-success',
                            showFeedback() && isSelected() && !isAnswer() && 'border-error bg-error-light animate-shake',
                            showFeedback() && !isSelected() && !isAnswer() && 'border-border opacity-50',
                            'disabled:cursor-default',
                          )}
                        >
                          <span class="absolute top-2 left-3 text-xs font-mono text-content-tertiary">{i() + 1}</span>
                          <p class="text-sm font-medium text-content pl-5">{opt}</p>
                        </button>
                      );
                    }}
                  </For>
                </div>

                {/* Feedback hint */}
                <Show when={phase() === 'feedback' && !isCorrect()}>
                  <div class="text-center">
                    <p class="text-sm text-error">
                      正确答案: <span class="font-semibold">{correctAnswer()}</span>
                    </p>
                  </div>
                </Show>
              </div>
            );
          }}
        </Show>
      </Show>

      {/* Summary */}
      <Show when={phase() === 'summary'}>
        <Card variant="elevated" class="text-center py-10 animate-scale-in">
          <div class="w-16 h-16 mx-auto mb-4 rounded-full bg-success-light flex items-center justify-center">
            <svg class="w-8 h-8 text-success" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
            </svg>
          </div>
          <h2 class="text-2xl font-bold text-content mb-2">学习完成!</h2>
          <div class="grid grid-cols-3 gap-4 my-6 max-w-sm mx-auto">
            <div>
              <p class="text-2xl font-bold text-accent">{totalQuestions()}</p>
              <p class="text-xs text-content-secondary">总答题数</p>
            </div>
            <div>
              <p class="text-2xl font-bold text-success">
                {totalQuestions() > 0 ? Math.round((correctCount() / totalQuestions()) * 100) : 0}%
              </p>
              <p class="text-xs text-content-secondary">正确率</p>
            </div>
            <div>
              <p class="text-2xl font-bold text-warning">{queue.masteredCount()}</p>
              <p class="text-xs text-content-secondary">已掌握</p>
            </div>
          </div>
          <div class="flex gap-3 justify-center">
            <Button onClick={() => navigate('/')} variant="outline">返回首页</Button>
            <Button onClick={restartSession}>再学一组</Button>
          </div>
        </Card>
      </Show>

      {/* 摄像头权限引导弹窗 */}
      <CameraPermission
        open={showCameraPermission()}
        onClose={() => setShowCameraPermission(false)}
        onConfirm={() => {
          setShowCameraPermission(false);
          fatigueStore.enable();
          startDetection();
        }}
      />

      {/* 疲劳警告弹窗 */}
      <FatigueWarningModal
        open={showFatigueWarning()}
        onClose={() => setShowFatigueWarning(false)}
        onRest={() => {
          setShowFatigueWarning(false);
          navigate('/');
        }}
      />
    </div>
  );
}
