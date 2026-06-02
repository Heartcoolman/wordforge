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
}

export const notificationsApi = {
  /** 收件箱列表 + 未读计数。unread=true 时仅返回未读。 */
  list: (unread?: boolean) =>
    api.get<AdminAlertInbox>(
      '/api/admin/notifications',
      unread ? { unread: true } : undefined,
      { useAdminToken: true },
    ),
  /** 标记单条已读(幂等),返回最新未读计数。 */
  markRead: (id: string) =>
    api.post<{ read: boolean; unreadCount: number }>(
      `/api/admin/notifications/${encodeURIComponent(id)}/read`,
      undefined,
      { useAdminToken: true },
    ),
};
