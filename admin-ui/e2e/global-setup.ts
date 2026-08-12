/**
 * e2e 全局前置：等待真后端就绪并幂等初始化 admin 账号。
 * 除 admin-real-flows（全 mock）外的套件都打真后端，且假定实例已初始化
 * （未初始化时 /admin/login 会被重定向到 /admin/setup，结构断言全数失败）。
 * setup 端点仅空 admin 表可用，重复调用报错——忽略即幂等。
 */
export default async function globalSetup() {
  const base = process.env.E2E_BACKEND_URL ?? 'http://127.0.0.1:3000';
  let healthy = false;
  for (let i = 0; i < 60; i++) {
    try {
      const res = await fetch(`${base}/health`, { headers: { Accept: 'application/json' } });
      if (res.ok) {
        healthy = true;
        break;
      }
    } catch {
      /* 未就绪，继续等 */
    }
    await new Promise((r) => setTimeout(r, 2000));
  }
  if (!healthy) {
    throw new Error(`e2e 后端 ${base} 未就绪：请先启动 learning-backend（见 .github/workflows/e2e-tests.yml）`);
  }
  await fetch(`${base}/api/admin/auth/setup`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email: 'e2e-admin@test.local', password: 'E2eAdminPassw0rd!' }),
  }).catch(() => {
    /* 已初始化 → 端点报错，幂等忽略 */
  });
}
