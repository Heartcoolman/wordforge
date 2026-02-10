import { api } from './client';
import type { StudyConfig, UpdateStudyConfigRequest, StudyProgress } from '@/types/studyConfig';
import type { Word } from '@/types/word';

export const studyConfigApi = {
  get() {
    return api.get<StudyConfig>('/api/study-config');
  },

  update(data: UpdateStudyConfigRequest) {
    return api.put<StudyConfig>('/api/study-config', data);
  },

  getTodayWords() {
    return api.get<{ words: Word[]; target: number }>('/api/study-config/today-words');
  },

  getProgress() {
    return api.get<StudyProgress>('/api/study-config/progress');
  },
};
