import { api } from './client';

export type RewardPreference = 'standard' | 'explorer' | 'achiever' | 'social';

export interface CognitiveProfile {
  attention: number;
  fatigue: number;
  motivation: number;
  confidence: number;
}

export interface LearningStyle {
  style: string;
  visual: number;
  auditory: number;
  reading: number;
  kinesthetic: number;
}

export interface Chronotype {
  type: 'morning' | 'evening' | 'neutral';
  preferredHours: number[];
}

export interface HabitProfile {
  preferredTimeSlot: string;
  medianSessionMinutes: number;
  dailyFrequency: number;
}

export const userProfileApi = {
  getReward: () => api.get<{ preference: RewardPreference }>('/api/user-profile/reward'),
  updateReward: (preference: RewardPreference) => api.put<{ preference: RewardPreference }>('/api/user-profile/reward', { preference }),
  getCognitive: () => api.get<CognitiveProfile>('/api/user-profile/cognitive'),
  getLearningStyle: () => api.get<LearningStyle>('/api/user-profile/learning-style'),
  getChronotype: () => api.get<Chronotype>('/api/user-profile/chronotype'),
  getHabit: () => api.get<HabitProfile>('/api/user-profile/habit'),
  updateHabit: (data: Partial<HabitProfile>) => api.post<HabitProfile>('/api/user-profile/habit', data),
  uploadAvatar: (file: File) => {
    // Use api.post with FormData — pass raw body without JSON stringification
    const formData = new FormData();
    formData.append('avatar', file);
    return api.post<{ avatarUrl: string }>('/api/user-profile/avatar', undefined, {
      body: formData,
      // Let browser set Content-Type with boundary automatically
      headers: {},
    });
  },
};
