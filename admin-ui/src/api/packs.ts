import { api, buildUrl } from './http';
import { tokenManager } from '@/lib/token';

/** 资源包通道。与后端 ResourcePackChannel 对齐（小写 stable / beta / internal） */
export type PackChannel = 'stable' | 'beta' | 'internal';

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

/** 各通道当前激活版本号（取自 resource_pack_active 表，无则 null） */
export interface PackActiveByChannel {
  stable: string | null;
  beta: string | null;
  internal: string | null;
}

/** 近 7 天该 pack 的安装结果计数（install_log 真实五态，m070 起） */
export interface PackOutcomes7d {
  installed: number;
  verify_failed: number;
  rollback: number;
  download_failed: number;
  apply_failed: number;
}

/** 列表 API 返回的每个 pack（含所有版本 + 激活态 + 安装聚合） */
export interface AdminPackEntry extends ResourcePack {
  versions: ResourcePackVersion[];
  /** 各通道当前激活版本号 */
  active: PackActiveByChannel;
  /** install_log 全量记录数 */
  totalInstalls: number;
  /** 近 7 天各 outcome 计数 */
  outcomes7d: PackOutcomes7d;
}

export interface PackStatsEntry {
  version: string;
  outcome: string;
  count: number;
}

/** 跨包全局聚合（GET /summary） */
export interface PackSummary {
  totalPacks: number;
  newPacksThisMonth: number;
  totalVersions: number;
  versionsByChannel: { stable: number; beta: number; internal: number };
  installsToday: number;
  installsTodaySuccess: number;
  /** 近 7 天 非installed/total，0..1；无数据为 0 */
  failureRate7d: number;
  failures7dByOutcome: {
    verify_failed: number;
    rollback: number;
    download_failed: number;
    apply_failed: number;
  };
  /** 当前在线客户端数（在线 SSE 连接），切激活对话框「受众」展示 */
  onlineClients: number;
}

export interface UploadQuery {
  /** 必填，新版本号 */
  version: string;
  /** stable / beta / internal */
  channel: PackChannel;
  /** 可选，最低兼容 app 版本 */
  minAppVersion?: string;
  /** 可选，本版本描述 */
  description?: string;
  /** 工件类型：默认 json（内容包）；web 客户端构建须传 tarball（payload.tar.gz，上限 128 MiB） */
  artifactType?: 'json' | 'tarball';
}

/**
 * Admin 资源包 API 封装。对接 src/routes/admin/resource_packs.rs（router prefix `/api/admin/resource-packs`）。
 *
 * 全部 endpoint 要求 admin token（http.ts 已通过 useAdminToken 注入）。
 */
export const packsApi = {
  /** GET / — 列出全部 pack + 每个 pack 的所有版本（含 active / totalInstalls / outcomes7d）。
   *  注意 api.get(path, params?, opts?)：useAdminToken 必须在第 3 个 opts 参数，否则会被当 query 序列化掉、Authorization 不附加。 */
  list: () => api.get<AdminPackEntry[]>('/api/admin/resource-packs', undefined, { useAdminToken: true }),

  /** GET /summary — 跨包全局聚合（KPI 卡数据源） */
  summary: () => api.get<PackSummary>('/api/admin/resource-packs/summary', undefined, { useAdminToken: true }),

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
  ): Promise<{ packId: string; version: string; sha256: string; signature: string; sizeBytes: number; channel: PackChannel; artifactType?: string }> => {
    // 经 buildUrl 解析，与其余请求一致遵循 API_BASE（VITE_API_BASE_URL）；
    // 否则原始相对路径会落到 window.location.origin，跨域配置下打到错误源。
    const uploadUrl = buildUrl(`/api/admin/resource-packs/${encodeURIComponent(packId)}/versions`, {
      version: query.version,
      channel: query.channel,
      minAppVersion: query.minAppVersion || undefined,
      description: query.description || undefined,
      // 默认 json；web 发版传 tarball，后端据此落盘 payload.tar.gz 并在激活时解包托管
      artifactType: query.artifactType || undefined,
    });

    // 用原生 XHR，axios/fetch 在 Solid 项目暂未引入上传进度封装
    return new Promise((resolve, reject) => {
      const xhr = new XMLHttpRequest();
      xhr.open('POST', uploadUrl);
      // token 经 tokenManager 读取（sessionStorage 原始串），不要用 localStorage+JSON.parse（存储格式是裸串，会抛异常导致无鉴权）
      const adminToken = tokenManager.getAdminToken();
      if (adminToken) {
        xhr.setRequestHeader('Authorization', `Bearer ${adminToken}`);
      }
      xhr.responseType = 'json';
      xhr.upload.onprogress = (e) => {
        if (e.lengthComputable && onProgress) onProgress(e.loaded / e.total);
      };
      xhr.onload = () => {
        if (xhr.status >= 200 && xhr.status < 300) {
          resolve(xhr.response?.data ?? xhr.response);
        } else {
          // 该 XHR 绕过了 http.ts 的统一管线，需自行处理 admin token 401，
          // 与 unwrap() 一致：清 admin token 并广播 admin:unauthorized 触发重新登录。
          if (xhr.status === 401) {
            tokenManager.clearAdminToken();
            window.dispatchEvent(new Event('admin:unauthorized'));
          }
          reject(new Error(xhr.response?.message ?? `HTTP ${xhr.status}`));
        }
      };
      xhr.onerror = () => reject(new Error('网络错误'));
      xhr.send(body as any);
    });
  },

  /** PUT /:packId/channel/:channel/active — 把指定版本设为当前通道激活版本，触发 SSE 广播。
   *  be:broadcast-packs-minor:响应新增 audienceClients（当前在线 SSE 连接数）供"受众客户端数"展示。 */
  setActive: (packId: string, channel: PackChannel, version: string) =>
    api.put<{ packId: string; channel: PackChannel; version: string; activated: boolean; audienceClients: number }>(
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

  /** GET /:packId/stats — 客户端 telemetry 按 (version, outcome) 聚合，后端包一层 {packId, stats} */
  stats: (packId: string) =>
    api.get<{ packId: string; stats: PackStatsEntry[] }>(
      `/api/admin/resource-packs/${encodeURIComponent(packId)}/stats`,
      undefined,
      { useAdminToken: true },
    ),
};
