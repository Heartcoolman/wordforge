/// Empty user lock slots are cleaned once the lock map grows beyond this size.
pub const USER_LOCK_CLEANUP_THRESHOLD: usize = 500;

/// Binary feedback above this value is treated as a positive learning signal.
pub const SIGNAL_THRESHOLD: f64 = 0.5;

/// Neutral baseline used when converting raw accuracy and speed into trend deltas.
pub const TREND_BASELINE: f64 = 0.5;

/// Highest difficulty allowed when the user is already strongly fatigued.
pub const FATIGUE_DIFFICULTY_CAP: f64 = 0.4;

/// Highest batch size allowed when the user is already strongly fatigued.
pub const FATIGUE_BATCH_SIZE_CAP: u32 = 5;

/// Highest new-word ratio allowed when the user is already strongly fatigued.
pub const FATIGUE_NEW_RATIO_CAP: f64 = 0.1;

/// Difficulty floor after a low-accuracy answer, so one miss does not make the next item trivial.
pub const LOW_ACCURACY_DIFFICULTY_FLOOR: f64 = 0.1;

/// Difficulty floor for low-motivation sessions, keeping review useful without overloading users.
pub const LOW_MOTIVATION_DIFFICULTY_FLOOR: f64 = 0.2;
