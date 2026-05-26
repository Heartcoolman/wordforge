/**
 * 客户端 / 服务器时钟对齐。
 *
 * **要解决的问题**：
 * 浏览器 `Date.now()` 用的是用户系统时间；服务器签 JWT 时用的是服务器本地时间。
 * 当两者偏差较大（典型如：boxd 类 microVM 在 suspend/resume 后时钟漂移数小时），
 * 前端 `isTokenExpired` 用客户端时间比 `exp` 会判错，触发"刚签发就过期"的死循环。
 *
 * **机制**：
 * 后端每个响应注入 `X-Server-Time: <unix_sec>` header（见 middleware/server_time.rs）。
 * 本模块从响应里读取该值，计算 `skew = server_time - client_time` 并持久化。
 * 之后任何与时间相关的判断改用 `nowSecs()` —— 它返回 `Date.now()/1000 + skew`，
 * 与服务器视角对齐。
 *
 * 即使无网络 / 服务器宕机，落地的 skew 也能持续生效到下次成功响应；网络恢复后
 * 第一个响应自动校正。
 */

const STORAGE_KEY = 'eng_clock_skew';
/** skew 超过此秒数视为过期，下次请求来前不参与判断（防止超久旧值误导）。 */
const STALE_AFTER_SECS = 86_400; // 24h
/** 容忍的最小服务器响应时间格式校验：合理 Unix 秒数范围（2024-01-01 起）。 */
const MIN_VALID_UNIX = 1_704_067_200;

interface StoredSkew {
  /** server_time - client_time，秒 */
  skew: number;
  /** 写入时的客户端 Unix 秒数 */
  setAt: number;
}

let inMemory: StoredSkew | null = null;

function loadFromStorage(): StoredSkew | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<StoredSkew>;
    if (typeof parsed.skew !== 'number' || typeof parsed.setAt !== 'number') return null;
    return { skew: parsed.skew, setAt: parsed.setAt };
  } catch {
    return null;
  }
}

function persist(value: StoredSkew): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
  } catch {
    /* quota exceeded / disabled storage：忽略，仍能在内存中使用 */
  }
}

function currentSkew(): number {
  if (!inMemory) inMemory = loadFromStorage();
  if (!inMemory) return 0;
  // 太旧的 skew 不再可信（用户机器或服务器都可能在期间被调过时间）
  const clientNow = Math.floor(Date.now() / 1000);
  if (clientNow - inMemory.setAt > STALE_AFTER_SECS) return 0;
  return inMemory.skew;
}

/**
 * 服务器时间锚定的"当前秒数"。所有 token/exp 类判断都应该用它而不是 Date.now()/1000。
 */
export function nowSecs(): number {
  return Math.floor(Date.now() / 1000) + currentSkew();
}

/**
 * 用一个服务器返回的 unix 时间戳更新 skew（一般由 fetch 拦截器在收到响应后调）。
 * 非法值（如非数字、过小）安全忽略。
 */
export function updateSkewFromServerTime(serverUnixSecs: number): void {
  if (!Number.isFinite(serverUnixSecs) || serverUnixSecs < MIN_VALID_UNIX) return;
  const clientNow = Math.floor(Date.now() / 1000);
  const skew = Math.round(serverUnixSecs - clientNow);
  const next: StoredSkew = { skew, setAt: clientNow };
  inMemory = next;
  persist(next);
}

/**
 * 从 Response 对象读 X-Server-Time header 并更新 skew。读不到/解析失败时安静返回。
 */
export function ingestServerTimeFromResponse(response: Response): void {
  const header = response.headers.get('X-Server-Time') ?? response.headers.get('x-server-time');
  if (!header) return;
  const parsed = Number.parseInt(header, 10);
  if (Number.isNaN(parsed)) return;
  updateSkewFromServerTime(parsed);
}

/** 当前已知 skew 秒数（测试 / 诊断用）。 */
export function getSkewSecs(): number {
  return currentSkew();
}

/** 测试 hook：重置内存与持久化状态。 */
export function _resetClockSkewForTests(): void {
  inMemory = null;
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    /* ignore */
  }
}
