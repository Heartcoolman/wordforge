import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { adminApi } from '@/api/admin';

describe('adminApi advisor/canary/whitelist 方法签名', () => {
  it('暴露 advisor 全套方法', () => {
    for (const m of [
      'amasAdvisorCost', 'amasAdvisorCostDaily', 'amasAdvisorRun', 'amasApproveAllSuggestions',
      'amasAdvisorConfig', 'amasUpdateAdvisorConfig', 'amasListWhitelist', 'amasAddWhitelist',
      'amasDeleteWhitelist', 'amasExportSuggestionsCsv', 'amasRollbackSuggestion',
      'amasListCanaries', 'amasCreateCanary', 'amasScaleCanary', 'amasRollbackCanary', 'amasPromoteCanary',
    ]) {
      expect(typeof (adminApi as unknown as Record<string, unknown>)[m]).toBe('function');
    }
  });

  describe('amasExportSuggestionsCsv', () => {
    const origFetch = globalThis.fetch;
    beforeEach(() => {
      globalThis.fetch = vi.fn().mockResolvedValue({
        ok: true,
        text: () => Promise.resolve('id,created_at\n1,2026-05-29'),
      } as unknown as Response);
    });
    afterEach(() => { globalThis.fetch = origFetch; });

    it('用 fetch 拿 csv 原文并带 status/q query', async () => {
      const csv = await adminApi.amasExportSuggestionsCsv('approved', 'memoryModel');
      expect(csv).toContain('id,created_at');
      const url = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0][0] as string;
      expect(url).toContain('/api/admin/amas/suggestions/export.csv');
      expect(url).toContain('status=approved');
      expect(url).toContain('q=memoryModel');
    });
  });
});
