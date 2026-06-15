import { api } from './http';

/**
 * D1 admin 应用内通知/告警收件箱。
 * 数据源是后端 system_alerts 表(AMAS 软拦截告警),经 /api/admin/notifications 透出。
 * **不是 end-user notifications**(那张表按 user_id 键控,本仓零消费且维度不同)。
 */
export interface AdminAlert {
  id: string;
  source: string;
  kind: string;
  severity: 'error' | 'warning' | 'info';
  title: string;
  message: string;
  count: number;
  firstSeenAt: string;
  lastSeenAt: string;
  /** 已读时间(null=未读)。 */
  readAt: string | null;
  /** 确认该告警的 admin id。 */
  ackedBy: string | null;
}

export interface AdminAlertInbox {
  items: AdminAlert[];
  unreadCount: number;
  /** 每页条数(后端 INBOX_LIMIT)。 */
  limit: number;
  /** 本次返回的分页偏移。offset + items.length < unreadCount(unread 视图)时仍有下一页。 */
  offset: number;
}

export const notificationsApi = {
  /** 收件箱列表(分页) + 未读计数。unread=true 仅返回未读;offset 翻页覆盖全部未读。 */
  list: (unread?: boolean, offset?: number) => {
    const params: Record<string, string | number | boolean | undefined> = {};
    if (unread) params.unread = true;
    if (offset && offset > 0) params.offset = offset;
    return api.get<AdminAlertInbox>(
      '/api/admin/notifications',
      Object.keys(params).length ? params : undefined,
      { useAdminToken: true },
    );
  },
  /** 标记单条已读(幂等),返回最新未读计数。 */
  markRead: (id: string) =>
    api.post<{ read: boolean; unreadCount: number }>(
      `/api/admin/notifications/${encodeURIComponent(id)}/read`,
      undefined,
      { useAdminToken: true },
    ),
  /** W2-3:一键全部已读,返回标记条数 + 重算后的未读计数。 */
  markAllRead: () =>
    api.post<{ marked: number; unreadCount: number }>(
      '/api/admin/notifications/read-all',
      undefined,
      { useAdminToken: true },
    ),
};
