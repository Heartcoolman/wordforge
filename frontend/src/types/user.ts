export interface User {
  id: string;
  email: string;
  username: string;
  isBanned: boolean;
}

export interface AuthResponse {
  accessToken: string;
  refreshToken: string;
  user: User;
}

export interface LoginRequest {
  email: string;
  password: string;
}

export interface RegisterRequest {
  email: string;
  username: string;
  password: string;
}

export interface UserStats {
  totalWordsLearned: number;
  totalSessions: number;
  totalRecords: number;
  streakDays: number;
  accuracyRate: number;
}

export interface ChangePasswordRequest {
  current_password: string;
  new_password: string;
}

export interface UserPreferences {
  theme: string;
  language: string;
  notificationEnabled: boolean;
  soundEnabled: boolean;
}
