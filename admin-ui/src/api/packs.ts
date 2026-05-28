import { api } from './http';

/** 资源包通道。与后端 ResourcePackChannel 对齐（小写） */
export type PackChannel = 'stable' | 'beta';

/** 单个资源包元数据 */
export interface ResourcePack {
  packId: string;
  description: string | null;
  createdAt: string;
  updatedAt: string;
}

/** 单个版本完整 row */
export interface ResourcePackVersion {
  packId: string;
  version: string;
  sha256: string;
  signature: string | null;
  signatureAlg: string;
  sizeBytes: number;
  minAppVersion: string | null;
  channel: PackChannel;
  payloadPath: string;
  publishedAt: string;
  deactivatedAt: string | null;
}

/** 列表 API 返回的每个 pack（含所有版本） */
export interface AdminPackEntry extends ResourcePack {
  versions: ResourcePackVersion[];
}

export interface PackStatsEntry {
  version: string;
  outcome: string;
  count: number;
}

export interface UploadQuery {
  /** 必填，新版本号 */
  version: string;
  /** stable / beta */
  channel: PackChannel;
  /** 可选，最低兼容 app 版本 */
  minAppVersion?: string;
  /** 可选，本版本描述 */
  description?: string;
}

/**
 * Admin 资源包 API 封装。对接 src/routes/admin/resource_packs.rs（router prefix `/api/admin/resource-packs`）。
 *
 * 全部 endpoint 要求 admin token（http.ts 已通过 useAdminToken 注入）。
 */
export const packsApi = {
  /** GET / — 列出全部 pack + 每个 pack 的所有版本 */
  list: () => api.get<AdminPackEntry[]>('/api/admin/resource-packs', { useAdminToken: true }),

  /**
   * POST /:packId/versions?version=&channel=&minAppVersion=&description=
   * raw body 是 payload bytes（Content-Type 不限，建议 application/json）。
   * 上传完成会自动 SHA256 + Ed25519 签名 + 落盘到 static/packs/<pack>/<version>/payload.json。
   *
   * @param onProgress 上传进度（0-1），用于 ResourcePacksPage 的 SSE 进度条
   */
  uploadVersion: (
    packId: string,
    query: UploadQuery,
    body: Blob | ArrayBuffer | string,
    onProgress?: (frac: number) => void,
  ): Promise<{ version: string; sha256: string; signatureAlg: string }> => {
    const params = new URLSearchParams({
      version: query.version,
      channel: query.channel,
    });
    if (query.minAppVersion) params.set('minAppVersion', query.minAppVersion);
    if (query.description) params.set('description', query.description);

    // 用原生 XHR，axios/fetch 在 Solid 项目暂未引入上传进度封装
    return new Promise((resolve, reject) => {
      const xhr = new XMLHttpRequest();
      xhr.open('POST', `/api/admin/resource-packs/${encodeURIComponent(packId)}/versions?${params.toString()}`);
      const adminToken = localStorage.getItem('eng_admin_token');
      if (adminToken) {
        xhr.setRequestHeader('Authorization', `Bearer ${JSON.parse(adminToken)}`);
      }
      xhr.responseType = 'json';
      xhr.upload.onprogress = (e) => {
        if (e.lengthComputable && onProgress) onProgress(e.loaded / e.total);
      };
      xhr.onload = () => {
        if (xhr.status >= 200 && xhr.status < 300) {
          resolve(xhr.response?.data ?? xhr.response);
        } else {
          reject(new Error(xhr.response?.message ?? `HTTP ${xhr.status}`));
        }
      };
      xhr.onerror = () => reject(new Error('网络错误'));
      xhr.send(body as any);
    });
  },

  /** PUT /:packId/channel/:channel/active — 把指定版本设为当前通道激活版本，触发 SSE 广播 */
  setActive: (packId: string, channel: PackChannel, version: string) =>
    api.put<{ activated: boolean; broadcasted: boolean }>(
      `/api/admin/resource-packs/${encodeURIComponent(packId)}/channel/${channel}/active`,
      { version },
      { useAdminToken: true },
    ),

  /** DELETE /:packId/versions/:version — 软删除（manifest 摘除，文件保留） */
  deactivateVersion: (packId: string, version: string) =>
    api.delete<{ deactivated: boolean }>(
      `/api/admin/resource-packs/${encodeURIComponent(packId)}/versions/${encodeURIComponent(version)}`,
      { useAdminToken: true },
    ),

  /** GET /:packId/stats — 客户端 telemetry 按 (version, outcome) 聚合 */
  stats: (packId: string) =>
    api.get<PackStatsEntry[]>(`/api/admin/resource-packs/${encodeURIComponent(packId)}/stats`, {
      useAdminToken: true,
    }),
};
