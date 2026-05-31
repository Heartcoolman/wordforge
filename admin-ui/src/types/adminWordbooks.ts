// 词库中心(本地系统+用户词库管理)契约类型。camelCase 镜像后端 serde rename_all=camelCase。
// 列表端点顶层用 items(照契约);分页型词条/历史端点用 paginated() 故数据字段为 data。

export type WordbookType = 'system' | 'user';

export interface WordbookSummary {
  id: string;
  name: string;
  description: string;
  type: WordbookType;
  userId?: string;
  ownerEmail?: string;
  wordCount: number;
  activeUsers: number;
  tags: string[];
  createdAt: string;
}

export interface WordbookListCounts {
  all: number;
  system: number;
  user: number;
  totalEntries: number;
}

export interface WordbookListResponse {
  items: WordbookSummary[];
  total: number;
  page: number;
  perPage: number;
  totalPages: number;
  counts: WordbookListCounts;
}

export interface WordbookStats {
  wordbookId: string;
  totalWords: number;
  activeUsers: number;
  /** 0..1 */
  avgMastery: number;
  weeklyAnswers: number;
}

export interface WordEntryRow {
  id: string;
  text: string;
  pronunciation?: string;
  partOfSpeech?: string;
  meaning: string;
  examples: string[];
  appearCount: number;
  /** 0..1;无记录为 null */
  accuracy?: number | null;
}

export interface WordEntriesPage {
  data: WordEntryRow[];
  total: number;
  page: number;
  perPage: number;
  totalPages: number;
}

export interface WordbookHeatmapCell {
  wordId: string;
  text: string;
  count: number;
}

export interface WordbookHeatmap {
  wordbookId: string;
  maxCount: number;
  cells: WordbookHeatmapCell[];
}

export interface WordbookDistributionBucket {
  label: string;
  min: number;
  max: number;
  userCount: number;
}

export interface WordbookUserDistribution {
  wordbookId: string;
  totalUsers: number;
  buckets: WordbookDistributionBucket[];
}

export interface WordbookAuditEntry {
  id: string;
  wordbookId: string;
  action: string;
  detail: string;
  adminId?: string;
  createdAt: string;
}

export interface WordbookAuditPage {
  data: WordbookAuditEntry[];
  total: number;
  page: number;
  perPage: number;
  totalPages: number;
}

/** POST/PATCH/DELETE 返回的 Wordbook 实体(后端 Wordbook 对象映射,字段宽松取用) */
export interface Wordbook {
  id: string;
  name: string;
  description: string;
  type: WordbookType;
  wordCount?: number;
  createdAt?: string;
}

export interface WordbookExport {
  id: string;
  name: string;
  description: string;
  type: WordbookType;
  version: string;
  words: Array<{
    spelling: string;
    phonetic?: string;
    partOfSpeech?: string;
    meaning: string;
    examples: string[];
  }>;
}

export type WordbookSortKey = 'newest' | 'hottest';
export type WordbookTypeFilter = 'all' | 'system' | 'user';
export type WordEntrySort = 'frequency' | 'alpha' | 'difficulty';

export interface AdminWordbooksQuery {
  type?: WordbookTypeFilter;
  search?: string;
  sort?: WordbookSortKey;
  page?: number;
  perPage?: number;
}

export interface AdminWordEntriesQuery {
  page?: number;
  perPage?: number;
  search?: string;
  pos?: string;
  sort?: WordEntrySort;
}

export interface CreateWordbookPayload {
  name: string;
  description?: string;
  type?: WordbookType;
}

export interface UpdateWordbookPayload {
  name?: string;
  description?: string;
}

export interface AddWordEntryPayload {
  text: string;
  pronunciation?: string;
  partOfSpeech?: string;
  meaning: string;
  examples?: string[];
}
