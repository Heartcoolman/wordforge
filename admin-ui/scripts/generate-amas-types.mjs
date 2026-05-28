// 由 schemars 导出的 JSON Schema 生成 AMAS 配置 TypeScript 类型。
//
// 数据流：
//   后端 Rust struct (#[derive(JsonSchema)])
//     ↓ cargo test --test amas_schema_export
//   admin-ui/src/types/amas.schema.json （checked-in，CI 单一事实源）
//     ↓ npm run gen:amas-types
//   admin-ui/src/types/amas.generated.ts
//
// 触发时机：后端 AMASConfig 或其子结构体增删字段时。
// CI 防漂移：在 cargo test 后跑 `npm run gen:amas-types && git diff --exit-code src/types/amas.generated.ts`。

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compile } from 'json-schema-to-typescript';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, '../..');
const schemaPath = resolve(repoRoot, 'admin-ui/src/types/amas.schema.json');
const outputPath = resolve(repoRoot, 'admin-ui/src/types/amas.generated.ts');

const schema = JSON.parse(readFileSync(schemaPath, 'utf8'));

const header = [
  '// AUTO-GENERATED — do not edit. Run: npm run gen:amas-types',
  '// Source: admin-ui/src/types/amas.schema.json (由后端 schemars 导出)',
  '// 后端事实源：src/amas/config.rs (AMASConfig) + 全部子结构体',
  '',
].join('\n');

const ts = await compile(schema, 'AMASConfig', {
  bannerComment: header,
  // 保留 schema 中的字段顺序，便于 review diff
  unreachableDefinitions: false,
  // 不要把 additionalProperties 自动加上 `[k: string]: unknown`
  additionalProperties: false,
  // 与项目其他 TS 一致
  style: { singleQuote: true, semi: true },
});

writeFileSync(outputPath, ts);
console.log(`Generated ${outputPath}`);
