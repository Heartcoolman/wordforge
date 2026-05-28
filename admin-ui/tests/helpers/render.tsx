import { render } from '@solidjs/testing-library';
import { Router, Route } from '@solidjs/router';
import type { JSX } from 'solid-js';

// 历史上曾经包了 @tanstack/solid-query 的 QueryClientProvider，但业务全部走
// SolidJS 原生 createResource（grep admin-ui/src 零处 useQuery / QueryClient），
// 已为 dead provider；且 package.json 没装 @tanstack/solid-query 导致 38 个测试
// 文件无法 transform 而整体 dark — 这里直接移除。
//
// 历史 commit 9a9673b 删了 package.json 依赖，但漏了这个 import；后续 dangling
// commit 7b0edbc 修过但未合入 main，导致 main 一直有 38 个 file-level dark tests。
// 此修复在 admin-ui rename PR 顺手清掉。

export function renderWithProviders(ui: () => JSX.Element) {
  return render(() => (
    <Router>
      <Route path="*" component={() => ui()} />
    </Router>
  ));
}
