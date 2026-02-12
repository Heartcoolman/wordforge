import { describe, it, expect, vi, beforeAll, afterAll, afterEach, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { http, HttpResponse } from 'msw';
import { renderWithProviders } from '../helpers/render';
import { server } from '../helpers/msw-server';
import LearningPage from '@/pages/LearningPage';
import { learningStore } from '@/stores/learning';
import { uiStore } from '@/stores/ui';

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

vi.mock('@/stores/learning', () => ({
  learningStore: {
    mode: vi.fn(() => 'word-to-meaning'),
    toggleMode: vi.fn(),
    startSession: vi.fn(),
    clearSession: vi.fn(),
  },
}));

vi.mock('@/stores/fatigue', () => ({
  fatigueStore: {
    detecting: vi.fn(() => false),
    fatigueLevel: vi.fn(() => 'none'),
    enable: vi.fn(),
    disable: vi.fn(),
  },
}));

vi.mock('@/hooks/useFatigueDetection', () => ({
  useFatigueDetection: () => ({
    start: vi.fn(),
    stop: vi.fn(),
  }),
}));

vi.mock('@/components/fatigue/FatigueToggle', () => ({
  FatigueToggle: () => <div data-testid="fatigue-toggle" />,
}));

vi.mock('@/components/fatigue/FatigueIndicator', () => ({
  FatigueIndicator: () => <div data-testid="fatigue-indicator" />,
}));

vi.mock('@/components/fatigue/FatigueWarningModal', () => ({
  FatigueWarningModal: () => null,
}));

vi.mock('@/components/fatigue/CameraPermission', () => ({
  CameraPermission: () => null,
}));

vi.mock('@solidjs/router', async (importOriginal) => {
  const mod = await importOriginal<typeof import('@solidjs/router')>();
  return { ...mod, useNavigate: () => vi.fn() };
});

const mockLearningStore = learningStore as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockUiStore = uiStore as unknown as { toast: Record<string, ReturnType<typeof vi.fn>> };

beforeAll(() => server.listen({ onUnhandledRequest: 'bypass' }));
beforeEach(() => { localStorage.clear(); });
afterEach(() => { server.resetHandlers(); vi.clearAllMocks(); });
afterAll(() => server.close());

// 辅助函数：点击目标数量按钮开始学习
async function clickGoalButton(goal: number) {
  // 查找 goal-setup 区域中的按钮
  const allButtons = screen.getAllByText(String(goal));
  // 找到包含 "个单词" 文本的按钮
  const goalBtn = allButtons
    .map((el) => el.closest('button'))
    .find((btn) => btn && btn.textContent?.includes('个单词'));
  if (!goalBtn) throw new Error(`Goal button for ${goal} not found`);
  fireEvent.click(goalBtn);
}

describe('LearningPage', () => {
  it('renders 单词学习 heading', () => {
    renderWithProviders(() => <LearningPage />);
    expect(screen.getByText('单词学习')).toBeInTheDocument();
  });

  it('shows goal-setup phase initially with goal buttons', () => {
    renderWithProviders(() => <LearningPage />);
    expect(screen.getByText('设定学习目标')).toBeInTheDocument();
    expect(screen.getByText('10')).toBeInTheDocument();
    expect(screen.getByText('15')).toBeInTheDocument();
    expect(screen.getByText('20')).toBeInTheDocument();
    expect(screen.getByText('30')).toBeInTheDocument();
  });

  it('shows loading spinner after clicking goal', async () => {
    server.use(
      http.post('/api/learning/session', () => new Promise(() => {})),
    );
    renderWithProviders(() => <LearningPage />);
    await clickGoalButton(10);
    await waitFor(() => {
      expect(screen.getByText('正在准备学习内容...')).toBeInTheDocument();
    });
  });

  it('shows mode toggle button', () => {
    renderWithProviders(() => <LearningPage />);
    expect(screen.getByText('英 → 中')).toBeInTheDocument();
  });

  it('mode toggle text shows 英 → 中 by default', () => {
    renderWithProviders(() => <LearningPage />);
    const toggle = screen.getByText('英 → 中');
    expect(toggle.tagName.toLowerCase()).toBe('button');
  });

  it('shows setup state when no words available', async () => {
    server.use(
      http.post('/api/learning/session', () =>
        HttpResponse.json({ success: true, data: { sessionId: 'test-session' } }),
      ),
      http.get('/api/learning/study-words', () =>
        HttpResponse.json({ success: true, data: { words: [], strategy: null } }),
      ),
    );
    renderWithProviders(() => <LearningPage />);
    await clickGoalButton(10);

    await waitFor(() => {
      expect(screen.getByText('准备开始学习')).toBeInTheDocument();
    }, { timeout: 3000 });
  });

  it('setup state has 管理词库 button', async () => {
    server.use(
      http.post('/api/learning/session', () =>
        HttpResponse.json({ success: true, data: { sessionId: 'test-session' } }),
      ),
      http.get('/api/learning/study-words', () =>
        HttpResponse.json({ success: true, data: { words: [], strategy: null } }),
      ),
    );
    renderWithProviders(() => <LearningPage />);
    await clickGoalButton(10);

    await waitFor(() => {
      expect(screen.getByText('管理词库')).toBeInTheDocument();
    }, { timeout: 3000 });
  });

  it('setup state has 选择词书 button', async () => {
    server.use(
      http.post('/api/learning/session', () =>
        HttpResponse.json({ success: true, data: { sessionId: 'test-session' } }),
      ),
      http.get('/api/learning/study-words', () =>
        HttpResponse.json({ success: true, data: { words: [], strategy: null } }),
      ),
    );
    renderWithProviders(() => <LearningPage />);
    await clickGoalButton(10);

    await waitFor(() => {
      expect(screen.getByText('选择词书')).toBeInTheDocument();
    }, { timeout: 3000 });
  });
});

/* ── Quiz phase tests ── */

const fakeWords = [
  { id: 'w1', text: 'apple', meaning: '苹果', difficulty: 3, examples: [], tags: [], createdAt: new Date().toISOString() },
  { id: 'w2', text: 'banana', meaning: '香蕉', difficulty: 3, examples: [], tags: [], createdAt: new Date().toISOString() },
  { id: 'w3', text: 'cherry', meaning: '樱桃', difficulty: 3, examples: [], tags: [], createdAt: new Date().toISOString() },
  { id: 'w4', text: 'grape', meaning: '葡萄', difficulty: 3, examples: [], tags: [], createdAt: new Date().toISOString() },
];

function setupQuizHandlers() {
  server.use(
    http.post('/api/learning/session', () =>
      HttpResponse.json({ success: true, data: { sessionId: 'test-session' } }),
    ),
    http.get('/api/learning/study-words', () =>
      HttpResponse.json({ success: true, data: { words: fakeWords, strategy: null } }),
    ),
    http.post('/api/records', () =>
      HttpResponse.json({ success: true, data: {} }),
    ),
    http.post('/api/learning/sync-progress', () =>
      HttpResponse.json({ success: true, data: {} }),
    ),
  );
}

async function startQuiz() {
  renderWithProviders(() => <LearningPage />);
  await clickGoalButton(10);
}

describe('LearningPage -- quiz phase', () => {
  beforeEach(() => {
    setupQuizHandlers();
  });

  it('shows a word text when quiz loads', async () => {
    await startQuiz();
    await waitFor(() => {
      const wordTexts = fakeWords.map(w => w.text);
      const found = wordTexts.some(t => screen.queryByText(t));
      expect(found).toBe(true);
    }, { timeout: 5000 });
  });

  it('shows 4 option buttons in quiz', async () => {
    await startQuiz();
    await waitFor(() => {
      expect(screen.getByText('1')).toBeInTheDocument();
      expect(screen.getByText('2')).toBeInTheDocument();
      expect(screen.getByText('3')).toBeInTheDocument();
      expect(screen.getByText('4')).toBeInTheDocument();
    }, { timeout: 5000 });
  });

  it('shows progress bar with mastered count', async () => {
    await startQuiz();
    await waitFor(() => {
      expect(screen.getByText(/已掌握/)).toBeInTheDocument();
    }, { timeout: 5000 });
  });

  it('shows question counter text', async () => {
    await startQuiz();
    await waitFor(() => {
      expect(screen.getByText(/正确率/)).toBeInTheDocument();
    }, { timeout: 5000 });
  });
});

describe('LearningPage -- answer handling', () => {
  beforeEach(() => {
    setupQuizHandlers();
  });

  it('clicking an option enters feedback phase', async () => {
    await startQuiz();

    // Wait for quiz to load
    await waitFor(() => {
      expect(screen.getByText('1')).toBeInTheDocument();
    }, { timeout: 5000 });

    // Click the first option button (any option)
    const firstOption = screen.getByText('1').closest('button')!;
    fireEvent.click(firstOption);

    // After clicking, buttons should be disabled (feedback phase)
    await waitFor(() => {
      const buttons = screen.getAllByText(/^[1-4]$/).map(el => el.closest('button'));
      const allDisabled = buttons.every(btn => btn?.hasAttribute('disabled'));
      expect(allDisabled).toBe(true);
    });
  });

  it('shows 正确答案: when wrong option clicked', async () => {
    await startQuiz();

    await waitFor(() => {
      expect(screen.getByText('1')).toBeInTheDocument();
    }, { timeout: 5000 });

    // Find which word is being shown
    const shownWord = fakeWords.find(w => screen.queryByText(w.text));
    expect(shownWord).toBeDefined();

    // Find a wrong option (one that is not the correct meaning)
    const allOptionButtons = screen.getAllByText(/^[1-4]$/).map(el => el.closest('button')!);
    const wrongButton = allOptionButtons.find(btn => {
      const optText = btn.querySelector('p')?.textContent;
      return optText !== shownWord!.meaning;
    });
    expect(wrongButton).toBeDefined();

    fireEvent.click(wrongButton!);

    await waitFor(() => {
      expect(screen.getByText(/正确答案:/)).toBeInTheDocument();
    });
  });
});

describe('LearningPage -- keyboard shortcuts', () => {
  beforeEach(() => {
    setupQuizHandlers();
  });

  it('pressing key 1 selects the first option', async () => {
    await startQuiz();

    await waitFor(() => {
      expect(screen.getByText('1')).toBeInTheDocument();
    }, { timeout: 5000 });

    // Press key '1' on document
    document.dispatchEvent(new KeyboardEvent('keydown', { key: '1', bubbles: true }));

    // After pressing, should enter feedback phase (buttons disabled)
    await waitFor(() => {
      const buttons = screen.getAllByText(/^[1-4]$/).map(el => el.closest('button'));
      const allDisabled = buttons.every(btn => btn?.hasAttribute('disabled'));
      expect(allDisabled).toBe(true);
    });
  });
});

describe('LearningPage -- mode toggle', () => {
  it('calls toggleMode when mode button clicked', () => {
    renderWithProviders(() => <LearningPage />);
    const btn = screen.getByText('英 → 中');
    fireEvent.click(btn);
    expect(mockLearningStore.toggleMode).toHaveBeenCalled();
  });

  it('shows 中 → 英 in meaning-to-word mode', () => {
    mockLearningStore.mode.mockReturnValue('meaning-to-word');
    renderWithProviders(() => <LearningPage />);
    expect(screen.getByText('中 → 英')).toBeInTheDocument();
  });
});

describe('LearningPage -- error handling', () => {
  it('shows error toast when session creation fails', async () => {
    server.use(
      http.post('/api/learning/session', () =>
        HttpResponse.json(
          { success: false, code: 'SERVER_ERROR', message: 'Server Error' },
          { status: 500 },
        ),
      ),
    );
    renderWithProviders(() => <LearningPage />);
    await clickGoalButton(10);
    await waitFor(() => {
      expect(mockUiStore.toast.error).toHaveBeenCalled();
    }, { timeout: 3000 });
  });

  it('shows setup phase after error', async () => {
    server.use(
      http.post('/api/learning/session', () =>
        HttpResponse.json(
          { success: false, code: 'SERVER_ERROR', message: 'Server Error' },
          { status: 500 },
        ),
      ),
    );
    renderWithProviders(() => <LearningPage />);
    await clickGoalButton(10);
    await waitFor(() => {
      expect(screen.getByText('准备开始学习')).toBeInTheDocument();
    }, { timeout: 3000 });
  });

  it('shows warning toast when no words available', async () => {
    server.use(
      http.post('/api/learning/session', () =>
        HttpResponse.json({ success: true, data: { sessionId: 'test-session' } }),
      ),
      http.get('/api/learning/study-words', () =>
        HttpResponse.json({ success: true, data: { words: [], strategy: null } }),
      ),
    );
    renderWithProviders(() => <LearningPage />);
    await clickGoalButton(10);
    await waitFor(() => {
      expect(mockUiStore.toast.warning).toHaveBeenCalledWith('暂无可学习的单词', '请先添加单词或选择词书');
    }, { timeout: 3000 });
  });
});
