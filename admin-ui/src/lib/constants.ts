// 密码最小长度
export const MIN_PASSWORD_LENGTH = 8;

// 分页
export const DEFAULT_PAGE_SIZE = 20;
export const MAX_PAGE_SIZE = 100;

// 延迟和冷却时间
export const TOAST_DURATION_MS = 3000;
export const TOAST_ERROR_DURATION_MS = 6000;
export const DEBOUNCE_DELAY_MS = 300;
export const COOLDOWN_MS = 60000;

// Token
export const TOKEN_REFRESH_BUFFER_SECS = 300;

// 学习相关
export const DEFAULT_BATCH_SIZE = 10;
export const MAX_DAILY_TARGET = 200;
export const MASTERY_THRESHOLD = 2; // 连续正确次数达到此值视为掌握
export const MAX_ANSWER_HISTORY = 1000;
export const RECENT_WINDOW_SIZE = 5;

// 疲劳警告冷却（FlashcardPage / LearningPage 共享）
export const FATIGUE_WARNING_COOLDOWN_MS = 5 * 60 * 1000;

// 学习页面
export const MASTERY_COMPLETE_DELAY_MS = 1500;
export const FEEDBACK_CORRECT_DELAY_MS = 1000;
export const FEEDBACK_INCORRECT_DELAY_MS = 2000;
export const GOAL_OPTIONS = [10, 15, 20, 30] as const;
export const MAX_CUSTOM_GOAL = 100;

// 登录节流
export const LOGIN_THROTTLE_THRESHOLD = 3;
export const LOGIN_MAX_COOLDOWN_MS = 30000;

// 统计页面
export const DAILY_CHART_DAYS = 14;
export const ACCURACY_HIGH_THRESHOLD = 0.8;
export const ACCURACY_MID_THRESHOLD = 0.5;

// 批量导入
export const IMPORT_BATCH_SIZE = 50;

// 管理员
export const ADMIN_MAX_LOCK_WAIT_SECS = 30;
export const SETTINGS_MAX_USERS = 100000;
export const SETTINGS_MAX_DAILY_WORDS = 500;
export const MONITORING_DEFAULT_LIMIT = 20;

// API 默认值
export const SEMANTIC_SEARCH_DEFAULT_LIMIT = 10;
export const AMAS_MONITORING_DEFAULT_LIMIT = 50;
export const WORD_STATES_DUE_DEFAULT_LIMIT = 50;

// 疲劳检测 WASM 参数
export const FATIGUE_EAR_THRESHOLD = 0.2;
export const FATIGUE_EAR_SMOOTH_WINDOW = 3;
export const FATIGUE_PERCLOS_THRESHOLD = 0.2;
export const FATIGUE_PERCLOS_WINDOW_SECS = 60;
export const FATIGUE_BLINK_CLOSE_THRESHOLD = 0.2;
export const FATIGUE_BLINK_OPEN_THRESHOLD = 0.25;
export const FATIGUE_YAWN_MAR_THRESHOLD = 0.6;
export const FATIGUE_HEAD_PITCH_THRESHOLD = 15.0;
export const FATIGUE_HEAD_ROLL_THRESHOLD = 20.0;

// 摄像头参数
export const CAMERA_WIDTH = 640;
export const CAMERA_HEIGHT = 480;
export const CAMERA_FRAME_RATE = 15;

// MediaPipe 模型 CDN 配置
export const MEDIAPIPE_CDN_URLS = [
  // 主要 CDN（国际）
  'https://cdn.jsdelivr.net/npm/@mediapipe/tasks-vision/wasm',
  // 备用 CDN
  'https://unpkg.com/@mediapipe/tasks-vision/wasm',
  // Google Storage（可能在中国大陆无法访问）
  'https://storage.googleapis.com/mediapipe-models',
] as const;

// MediaPipe 模型资源路径
export const MEDIAPIPE_MODEL_ASSET_PATH =
  'face_landmarker/face_landmarker/float16/1/face_landmarker.task';
