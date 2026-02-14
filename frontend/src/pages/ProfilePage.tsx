import { createSignal, Show, For, onMount } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Card } from '@/components/ui/Card';
import { Input } from '@/components/ui/Input';
import { Button } from '@/components/ui/Button';
import { Select } from '@/components/ui/Select';
import { ProgressBar } from '@/components/ui/Progress';
import { Badge as UiBadge } from '@/components/ui/Badge';
import { Spinner } from '@/components/ui/Spinner';
import { Skeleton, CardSkeleton } from '@/components/ui/Skeleton';
import { authStore } from '@/stores/auth';
import { uiStore } from '@/stores/ui';
import { usersApi } from '@/api/users';
import { userProfileApi } from '@/api/userProfile';
import { notificationsApi } from '@/api/notifications';
import { MIN_PASSWORD_LENGTH } from '@/lib/constants';
import type { RewardType, RewardPreference, CognitiveProfile, Chronotype, HabitProfile } from '@/types/userProfile';
import type { Badge } from '@/types/notification';

const REWARD_OPTIONS = [
  { value: 'standard', label: '标准模式' },
  { value: 'explorer', label: '探索者' },
  { value: 'achiever', label: '成就者' },
  { value: 'social', label: '社交型' },
];

const CHRONOTYPE_LABELS: Record<string, string> = {
  morning: '早起型',
  evening: '夜猫型',
  neutral: '均衡型',
};

export default function ProfilePage() {
  const navigate = useNavigate();
  const [username, setUsername] = createSignal('');
  const [saving, setSaving] = createSignal(false);
  const [currentPw, setCurrentPw] = createSignal('');
  const [newPw, setNewPw] = createSignal('');
  const [confirmPw, setConfirmPw] = createSignal('');
  const [changingPw, setChangingPw] = createSignal(false);

  // Avatar
  const [avatarUrl, setAvatarUrl] = createSignal('');
  const [uploadingAvatar, setUploadingAvatar] = createSignal(false);

  // Reward
  const [reward, setReward] = createSignal<RewardPreference | null>(null);
  const [rewardLoading, setRewardLoading] = createSignal(true);
  const [savingReward, setSavingReward] = createSignal(false);

  // Cognitive
  const [cognitive, setCognitive] = createSignal<CognitiveProfile | null>(null);
  const [cognitiveLoading, setCognitiveLoading] = createSignal(true);

  // Learning style (backend returns processingSpeed/memoryCapacity/stability)
  const [learningStyle, setLearningStyle] = createSignal<CognitiveProfile | null>(null);
  const [styleLoading, setStyleLoading] = createSignal(true);

  // Chronotype
  const [chronotype, setChronotype] = createSignal<Chronotype | null>(null);
  const [chronoLoading, setChronoLoading] = createSignal(true);

  // Habit
  const [habit, setHabit] = createSignal<HabitProfile | null>(null);
  const [habitLoading, setHabitLoading] = createSignal(true);
  const [savingHabit, setSavingHabit] = createSignal(false);
  const [habitHours, setHabitHours] = createSignal('');
  const [habitSessionLen, setHabitSessionLen] = createSignal('');
  const [habitSessionsPerDay, setHabitSessionsPerDay] = createSignal('');

  // Badges
  const [badges, setBadges] = createSignal<Badge[]>([]);
  const [badgesLoading, setBadgesLoading] = createSignal(true);

  onMount(() => {
    const u = authStore.user();
    if (u) setUsername(u.username);
    loadProfileData();
  });

  async function loadProfileData() {
    const results = await Promise.allSettled([
      userProfileApi.getReward(),
      userProfileApi.getCognitive(),
      userProfileApi.getLearningStyle(),
      userProfileApi.getChronotype(),
      userProfileApi.getHabit(),
      notificationsApi.getBadges(),
    ]);

    if (results[0].status === 'fulfilled') setReward(results[0].value);
    setRewardLoading(false);

    if (results[1].status === 'fulfilled') setCognitive(results[1].value);
    setCognitiveLoading(false);

    if (results[2].status === 'fulfilled') {
      setLearningStyle(results[2].value);
    }
    setStyleLoading(false);

    if (results[3].status === 'fulfilled') setChronotype(results[3].value);
    setChronoLoading(false);

    if (results[4].status === 'fulfilled') {
      const h = results[4].value;
      setHabit(h);
      setHabitHours(h.preferredHours?.join(', ') ?? '');
      setHabitSessionLen(String(h.medianSessionLengthMins ?? ''));
      setHabitSessionsPerDay(String(h.sessionsPerDay ?? ''));
    }
    setHabitLoading(false);

    if (results[5].status === 'fulfilled') setBadges(results[5].value);
    setBadgesLoading(false);
  }

  async function saveUsername() {
    if (!username().trim()) return;
    setSaving(true);
    try {
      const updated = await usersApi.updateMe({ username: username() });
      authStore.updateUser(updated);
      uiStore.toast.success('用户名已更新');
    } catch (err: unknown) {
      uiStore.toast.error('更新失败', err instanceof Error ? err.message : '');
    } finally {
      setSaving(false);
    }
  }

  async function changePassword() {
    if (!currentPw() || !newPw()) return;
    if (newPw().length < MIN_PASSWORD_LENGTH) { uiStore.toast.error(`新密码至少 ${MIN_PASSWORD_LENGTH} 位`); return; }
    if (newPw() !== confirmPw()) { uiStore.toast.error('两次密码不一致'); return; }
    setChangingPw(true);
    try {
      await usersApi.changePassword({ currentPassword: currentPw(), newPassword: newPw() });
      uiStore.toast.success('密码已修改');
      setCurrentPw(''); setNewPw(''); setConfirmPw('');
    } catch (err: unknown) {
      uiStore.toast.error('修改失败', err instanceof Error ? err.message : '');
    } finally {
      setChangingPw(false);
    }
  }

  async function handleAvatarUpload(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    setUploadingAvatar(true);
    try {
      const res = await userProfileApi.uploadAvatar(file);
      setAvatarUrl(res.avatarUrl);
      uiStore.toast.success('头像已更新');
    } catch (err: unknown) {
      uiStore.toast.error('头像上传失败', err instanceof Error ? err.message : '');
    } finally {
      setUploadingAvatar(false);
      input.value = '';
    }
  }

  async function saveReward(val: string) {
    setSavingReward(true);
    try {
      const updated = await userProfileApi.updateReward(val as RewardType);
      setReward(updated);
      uiStore.toast.success('奖励偏好已更新');
    } catch (err: unknown) {
      uiStore.toast.error('更新失败', err instanceof Error ? err.message : '');
    } finally {
      setSavingReward(false);
    }
  }

  async function saveHabit() {
    setSavingHabit(true);
    try {
      const hours = habitHours().split(',').map(s => parseInt(s.trim())).filter(n => !isNaN(n));
      const sessionLen = parseFloat(habitSessionLen());
      const spd = parseFloat(habitSessionsPerDay());
      const updated = await userProfileApi.updateHabit({
        preferredHours: hours.length > 0 ? hours : undefined,
        medianSessionLengthMins: isNaN(sessionLen) ? undefined : sessionLen,
        sessionsPerDay: isNaN(spd) ? undefined : spd,
      });
      setHabit(updated);
      uiStore.toast.success('学习习惯已更新');
    } catch (err: unknown) {
      uiStore.toast.error('更新失败', err instanceof Error ? err.message : '');
    } finally {
      setSavingHabit(false);
    }
  }

  async function handleLogout() {
    await authStore.logout();
    navigate('/login', { replace: true });
  }

  return (
    <div class="max-w-2xl mx-auto space-y-6 animate-fade-in-up">
      <h1 class="text-2xl font-bold text-content">个人中心</h1>

      {/* 基本信息 + 头像 */}
      <Show when={authStore.user()} fallback={
        <Card variant="elevated"><p class="text-center text-content-secondary py-4">加载中...</p></Card>
      }>
        {(user) => (
          <Card variant="elevated">
            <div class="space-y-4">
              <div class="flex items-center gap-4">
                <div class="relative">
                  <div class="w-16 h-16 rounded-full bg-surface-tertiary flex items-center justify-center overflow-hidden border-2 border-border">
                    <Show when={avatarUrl()} fallback={
                      <span class="text-2xl font-bold text-content-secondary">
                        {user().username?.charAt(0)?.toUpperCase() || user().email?.charAt(0)?.toUpperCase()}
                      </span>
                    }>
                      <img src={avatarUrl()} alt="avatar" class="w-full h-full object-cover" />
                    </Show>
                  </div>
                  <label class="absolute -bottom-1 -right-1 w-6 h-6 rounded-full bg-accent text-white flex items-center justify-center cursor-pointer hover:bg-accent/80 transition-colors">
                    <Show when={!uploadingAvatar()} fallback={<Spinner size="sm" class="text-white" />}>
                      <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path d="M12 4v16m8-8H4" />
                      </svg>
                    </Show>
                    <input type="file" accept="image/png,image/jpeg,image/gif,image/webp" class="hidden" onChange={handleAvatarUpload} disabled={uploadingAvatar()} />
                  </label>
                </div>
                <div class="flex-1 min-w-0">
                  <p class="text-sm text-content-secondary">邮箱</p>
                  <p class="text-content font-medium truncate">{user().email}</p>
                </div>
              </div>
              <Input label="用户名" value={username()} onInput={(e) => setUsername(e.currentTarget.value)} />
              <Button onClick={saveUsername} loading={saving()} size="sm">保存</Button>
            </div>
          </Card>
        )}
      </Show>

      {/* 成就徽章 */}
      <Card variant="elevated">
        <h2 class="text-lg font-semibold text-content mb-4">成就徽章</h2>
        <Show when={!badgesLoading()} fallback={
          <div class="grid grid-cols-1 sm:grid-cols-3 gap-3">
            <Skeleton height="6rem" /><Skeleton height="6rem" /><Skeleton height="6rem" />
          </div>
        }>
          <Show when={badges().length > 0} fallback={
            <p class="text-sm text-content-secondary text-center py-4">暂无徽章数据</p>
          }>
            <div class="grid grid-cols-1 sm:grid-cols-3 gap-3">
              <For each={badges()}>
                {(badge) => (
                  <div class={`rounded-lg p-3 border transition-all ${badge.unlocked ? 'border-accent bg-accent/5' : 'border-border bg-surface-secondary opacity-70'}`}>
                    <div class="flex items-center justify-between mb-2">
                      <span class="text-sm font-medium text-content">{badge.name}</span>
                      <Show when={badge.unlocked}>
                        <UiBadge variant="success" size="sm">已解锁</UiBadge>
                      </Show>
                    </div>
                    <p class="text-xs text-content-secondary mb-2">{badge.description}</p>
                    <ProgressBar
                      value={Math.round(badge.progress * 100)}
                      size="sm"
                      color={badge.unlocked ? 'success' : 'accent'}
                    />
                    <Show when={badge.unlockedAt}>
                      <p class="text-[10px] text-content-secondary mt-1">
                        {new Date(badge.unlockedAt!).toLocaleDateString('zh-CN')}
                      </p>
                    </Show>
                  </div>
                )}
              </For>
            </div>
          </Show>
        </Show>
      </Card>

      {/* 奖励偏好 */}
      <Card variant="elevated">
        <h2 class="text-lg font-semibold text-content mb-4">奖励偏好</h2>
        <Show when={!rewardLoading()} fallback={<Skeleton height="2.5rem" />}>
          <Select
            label="选择奖励类型"
            options={REWARD_OPTIONS}
            value={reward()?.rewardType ?? 'standard'}
            onChange={(e) => saveReward(e.currentTarget.value)}
            disabled={savingReward()}
          />
          <Show when={savingReward()}>
            <p class="text-xs text-content-secondary mt-1">保存中...</p>
          </Show>
        </Show>
      </Card>

      {/* 认知档案 */}
      <Card variant="elevated">
        <h2 class="text-lg font-semibold text-content mb-4">认知档案</h2>
        <Show when={!cognitiveLoading()} fallback={<Skeleton height="4rem" />}>
          <Show when={cognitive()} fallback={
            <p class="text-sm text-content-secondary">暂无数据，完成更多学习后自动生成</p>
          }>
            {(cp) => (
              <div class="space-y-3">
                <ProfileMetric label="记忆容量" value={cp().memoryCapacity} />
                <ProfileMetric label="处理速度" value={cp().processingSpeed} />
                <ProfileMetric label="稳定性" value={cp().stability} />
              </div>
            )}
          </Show>
        </Show>
      </Card>

      {/* 学习风格 */}
      <Card variant="elevated">
        <h2 class="text-lg font-semibold text-content mb-4">学习风格</h2>
        <Show when={!styleLoading()} fallback={<Skeleton height="4rem" />}>
          <Show when={learningStyle()} fallback={
            <p class="text-sm text-content-secondary">暂无数据，完成更多学习后自动生成</p>
          }>
            {(ls) => (
              <div class="space-y-3">
                <ProfileMetric label="处理速度" value={ls().processingSpeed} />
                <ProfileMetric label="记忆容量" value={ls().memoryCapacity} />
                <ProfileMetric label="稳定性" value={ls().stability} />
              </div>
            )}
          </Show>
        </Show>
      </Card>

      {/* 时型偏好 */}
      <Card variant="elevated">
        <h2 class="text-lg font-semibold text-content mb-4">时型偏好</h2>
        <Show when={!chronoLoading()} fallback={<Skeleton height="3rem" />}>
          <Show when={chronotype()} fallback={
            <p class="text-sm text-content-secondary">暂无数据</p>
          }>
            {(ct) => (
              <div class="space-y-2">
                <div class="flex items-center gap-2">
                  <span class="text-sm text-content-secondary">类型：</span>
                  <UiBadge variant="accent">{CHRONOTYPE_LABELS[ct().chronotype] ?? ct().chronotype}</UiBadge>
                </div>
                <div>
                  <span class="text-sm text-content-secondary">偏好时段：</span>
                  <span class="text-sm text-content">{ct().preferredHours?.map(h => `${h}:00`).join(', ') || '未设定'}</span>
                </div>
              </div>
            )}
          </Show>
        </Show>
      </Card>

      {/* 学习习惯 */}
      <Card variant="elevated">
        <h2 class="text-lg font-semibold text-content mb-4">学习习惯</h2>
        <Show when={!habitLoading()} fallback={<Skeleton height="6rem" />}>
          <div class="space-y-3">
            <Input
              label="偏好学习时段（小时，逗号分隔，如 8,12,20）"
              value={habitHours()}
              onInput={(e) => setHabitHours(e.currentTarget.value)}
              placeholder="8, 12, 20"
            />
            <Input
              label="每次学习时长（分钟）"
              type="number"
              value={habitSessionLen()}
              onInput={(e) => setHabitSessionLen(e.currentTarget.value)}
              placeholder="15"
            />
            <Input
              label="每日学习次数"
              type="number"
              value={habitSessionsPerDay()}
              onInput={(e) => setHabitSessionsPerDay(e.currentTarget.value)}
              placeholder="1"
            />
            <Button onClick={saveHabit} loading={savingHabit()} size="sm">保存习惯</Button>
          </div>
        </Show>
      </Card>

      {/* 修改密码 */}
      <Card variant="elevated">
        <h2 class="text-lg font-semibold text-content mb-4">修改密码</h2>
        <div class="space-y-3">
          <Input label="当前密码" type="password" value={currentPw()} onInput={(e) => setCurrentPw(e.currentTarget.value)} />
          <Input label="新密码" type="password" placeholder={`至少 ${MIN_PASSWORD_LENGTH} 位`} value={newPw()} onInput={(e) => setNewPw(e.currentTarget.value)} />
          <Input label="确认新密码" type="password" placeholder="再次输入新密码" value={confirmPw()} onInput={(e) => setConfirmPw(e.currentTarget.value)} />
          <Button onClick={changePassword} loading={changingPw()} size="sm" variant="secondary">修改密码</Button>
        </div>
      </Card>

      {/* 退出 */}
      <Card variant="outlined">
        <Button onClick={handleLogout} variant="danger" fullWidth>退出登录</Button>
      </Card>
    </div>
  );
}

function ProfileMetric(props: { label: string; value: number }) {
  const display = () => {
    const v = props.value;
    if (v == null || isNaN(v)) return 0;
    return Math.round(v * 100);
  };

  return (
    <div>
      <div class="flex justify-between text-sm mb-1">
        <span class="text-content-secondary">{props.label}</span>
        <span class="text-content font-medium">{display()}%</span>
      </div>
      <ProgressBar value={display()} size="sm" color="accent" />
    </div>
  );
}
