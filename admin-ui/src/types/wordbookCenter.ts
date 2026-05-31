export interface RemoteWordbookMeta {
  id: string;
  name: string;
  description: string;
  wordCount: number;
  coverImage?: string;
  tags: string[];
  version: string;
  author?: string;
  downloadCount?: number;
}

export interface BrowseItem extends RemoteWordbookMeta {
  imported: boolean;
  localWordbookId?: string;
  localVersion?: string;
  hasUpdate: boolean;
  /** 本地标签(wordbook_local_tags),与远端 tags 区分;未导入则为空数组。标签编辑器用此预填。 */
  localTags: string[];
}

export interface RemoteWordPreview {
  spelling: string;
  phonetic?: string;
  meanings: string[];
  examples: string[];
}

export interface WordbookPreview {
  id: string;
  name: string;
  description: string;
  wordCount: number;
  coverImage?: string;
  tags: string[];
  version: string;
  author?: string;
  downloadCount?: number;
  words: {
    data: RemoteWordPreview[];
    total: number;
    page: number;
    perPage: number;
    totalPages: number;
  };
}

export interface ImportResult {
  wordbook: {
    id: string;
    name: string;
    description: string;
    type: 'system' | 'user';
    wordCount: number;
  };
  wordsImported: number;
  wordsSkipped: number;
}

export interface UpdateInfo {
  remoteId: string;
  name: string;
  localVersion: string;
  remoteVersion: string;
  localWordbookId: string;
}

export interface SyncResult {
  wordbook: {
    id: string;
    name: string;
    wordCount: number;
  };
  wordsAdded: number;
  wordsUpdated: number;
  wordsRemoved: number;
}

export interface UserWbCenterSettings {
  wordbookCenterUrl: string | null;
}
