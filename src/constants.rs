/// CAS（Compare-And-Swap）操作最大重试次数
pub const MAX_CAS_RETRIES: u32 = 20;

/// 连续登录失败达到此次数后锁定账户
pub const MAX_FAILED_LOGIN_ATTEMPTS: u32 = 5;

/// 账户锁定时长（分钟）
pub const LOCKOUT_DURATION_MINUTES: i64 = 15;

/// 默认每日学习单词数
pub const DEFAULT_DAILY_WORDS: u32 = 20;

/// 默认每日掌握目标
pub const DEFAULT_DAILY_MASTERY_TARGET: u32 = 10;

/// 系统默认最大用户数
pub const DEFAULT_MAX_USERS: u64 = 10_000;

/// 默认分页大小（records / v1 routes）
pub const DEFAULT_PAGE_SIZE_RECORDS: u64 = 50;

/// 列表接口默认分页大小
pub const DEFAULT_PAGE_SIZE: u64 = 20;

/// 列表接口最大分页大小
pub const MAX_PAGE_SIZE: u64 = 100;

/// 新单词初始半衰期（小时）
pub const DEFAULT_HALF_LIFE_HOURS: f64 = 24.0;

/// 默认习惯偏好学习时段
pub const DEFAULT_PREFERRED_HOURS: &[u8] = &[9, 14, 20];

/// 混淆对列表最大返回数量
pub const MAX_CONFUSION_PAIRS: usize = 100;

/// 默认用户偏好主题
pub const DEFAULT_THEME: &str = "light";

/// 默认用户偏好语言
pub const DEFAULT_LANGUAGE: &str = "en";

/// 每小时毫秒数
pub const MILLIS_PER_HOUR: i64 = 3_600_000;
