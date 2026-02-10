export type StudyMode = 'MASTERY' | 'REVIEW' | 'MIXED';

export interface StudyConfig {
  userId: string;
  selectedWordbookIds: string[];
  dailyWordCount: number;
  studyMode: StudyMode;
  dailyMasteryTarget: number;
}

export interface UpdateStudyConfigRequest {
  selectedWordbookIds?: string[];
  dailyWordCount?: number;
  studyMode?: StudyMode;
  dailyMasteryTarget?: number;
}

export interface StudyProgress {
  studied: number;
  target: number;
  new: number;
  learning: number;
  reviewing: number;
  mastered: number;
}
