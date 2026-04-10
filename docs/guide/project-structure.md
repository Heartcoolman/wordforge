# 项目结构

```
wordforge/
├── src/                          # Rust 后端
│   ├── main.rs                   # 入口
│   ├── config.rs                 # 全局配置（环境变量解析）
│   ├── state.rs                  # 应用状态定义
│   ├── amas/                     # AMAS 自适应算法引擎
│   │   ├── engine.rs             #   引擎核心调度
│   │   ├── elo.rs                #   ELO 评分系统
│   │   ├── config.rs             #   算法参数配置
│   │   ├── types.rs              #   类型定义
│   │   ├── word_selector.rs      #   智能选词
│   │   ├── metrics.rs            #   性能指标
│   │   ├── metrics_persistence.rs #  指标持久化
│   │   ├── monitoring.rs         #   引擎监控
│   │   ├── memory/               #   记忆模型
│   │   │   ├── mastery.rs        #     掌握度模型
│   │   │   ├── mdm.rs            #     多维记忆模型
│   │   │   ├── ssp.rs            #     SSP 间隔重复调度
│   │   │   ├── evm.rs            #     指数衰减模型
│   │   │   ├── iad.rs            #     间隔自适应模型
│   │   │   ├── mtp.rs            #     多轨迹预测模型
│   │   │   └── benchmark_adapter.rs #  性能基准适配器
│   │   └── decision/             #   决策层
│   │       ├── ige.rs            #     智能生成引擎
│   │       ├── ensemble.rs       #     集成决策
│   │       ├── heuristic.rs      #     启发式决策
│   │       └── swd.rs            #     选词决策
│   ├── routes/                   # API 路由
│   │   ├── auth.rs               #   用户认证
│   │   ├── learning.rs           #   学习流程
│   │   ├── words.rs              #   单词管理
│   │   ├── wordbooks.rs          #   词本管理
│   │   ├── wordbook_center.rs    #   词书中心
│   │   ├── records.rs            #   学习记录
│   │   ├── notifications.rs      #   通知系统
│   │   ├── user_profile.rs       #   用户画像
│   │   ├── content.rs            #   内容增强（词源、词素、混淆词）
│   │   ├── realtime.rs           #   SSE 实时推送
│   │   ├── health.rs             #   健康检查
│   │   ├── admin/                #   管理后台路由
│   │   └── ...
│   ├── workers/                  # 后台定时任务（17+）
│   │   ├── forgetting_alert.rs   #   遗忘预警
│   │   ├── daily_aggregation.rs  #   每日数据聚合
│   │   ├── delayed_reward.rs     #   延迟奖励计算
│   │   ├── confusion_pair_cache.rs #  混淆词对缓存
│   │   ├── word_clustering.rs    #   词汇聚类
│   │   ├── etymology_generation.rs # 词源生成
│   │   └── ...
│   ├── store/                    # 数据存储层（SQLite）
│   │   ├── mod.rs                #   Store 核心（连接池管理）
│   │   ├── schema.rs             #   数据表结构定义
│   │   ├── keys.rs               #   键名与校验
│   │   ├── migrate.rs            #   数据库迁移
│   │   └── operations/           #   按领域拆分的操作模块
│   │       ├── users.rs          #     用户操作
│   │       ├── sessions.rs       #     会话操作
│   │       ├── words.rs          #     单词操作
│   │       ├── wordbooks.rs      #     词本操作
│   │       ├── records.rs        #     学习记录操作
│   │       ├── notifications.rs  #     通知操作
│   │       ├── elo.rs            #     ELO 评分操作
│   │       ├── engine.rs         #     引擎状态操作
│   │       ├── extras.rs         #     扩展操作（徽章、词源、偏好等）
│   │       └── ...
│   ├── bin/                      # CLI 工具
│   │   ├── migrate_sled_to_sqlite.rs # Sled → SQLite 迁移工具
│   │   └── maimemo_mdm_adapter.rs    # 墨墨 MDM 数据适配器
│   ├── middleware/               # 中间件（速率限制、请求 ID）
│   └── services/                 # 业务服务
├── frontend/                     # SolidJS 前端
│   ├── src/
│   │   ├── pages/                # 页面组件
│   │   ├── components/           # UI 组件库
│   │   ├── api/                  # API 客户端
│   │   ├── stores/               # 状态管理
│   │   ├── lib/                  # 工具库
│   │   └── types/                # TypeScript 类型
│   └── tests/                    # 前端测试
├── tests/                        # 后端集成测试
│   ├── coverage_routes_http.rs   #   路由覆盖测试
│   ├── workers_coverage.rs       #   Workers 覆盖测试
│   ├── amas_effectiveness.rs     #   AMAS 效果测试
│   ├── amas_monte_carlo.rs       #   蒙特卡洛模拟测试
│   ├── property_memory_models.rs #   记忆模型属性测试（proptest）
│   └── ...
├── docs/                         # VitePress 文档站
├── static/                       # 静态资源 + SPA 入口
├── .env.example                  # 环境变量模板
└── Cargo.toml
```
