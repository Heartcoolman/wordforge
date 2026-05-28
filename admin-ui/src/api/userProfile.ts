import { api } from './http';
import type {
  RewardType,
  RewardPreference,
  CognitiveProfile,
  LearningStyle,
  Chronotype,
  HabitProfile,
  HabitProfileRequest,
} from '@/types/userProfile';

export const userProfileApi = {
  getReward: () => api.get<RewardPreference>('/api/user-profile/reward'),
  updateReward: (rewardType: RewardType) => api.put<RewardPreference>('/api/user-profile/reward', { rewardType }),
  getCognitive: () => api.get<CognitiveProfile>('/api/user-profile/cognitive'),
  getLearningStyle: () => api.get<LearningStyle>('/api/user-profile/learning-style'),
  getChronotype: () => api.get<Chronotype>('/api/user-profile/chronotype'),
  getHabit: () => api.get<HabitProfile>('/api/user-profile/habit'),
  updateHabit: (data: HabitProfileRequest) => {
    const sanitized = { ...data };
    if (sanitized.preferredHours) sanitized.preferredHours = sanitized.preferredHours.map(h => Math.round(h));
    if (sanitized.medianSessionLengthMins != null) sanitized.medianSessionLengthMins = Math.round(sanitized.medianSessionLengthMins);
    return api.post<HabitProfile>('/api/user-profile/habit', sanitized);
  },
  uploadAvatar: async (file: File) => {
    const buffer = await file.arrayBuffer();
    return api.post<{ avatarUrl: string }>('/api/user-profile/avatar', undefined, {
      body: buffer,
      headers: { 'Content-Type': file.type || 'application/octet-stream' },
    });
  },
};
