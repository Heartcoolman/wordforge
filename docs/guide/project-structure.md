# 项目结构

```
english/
├── src/                          # Rust 后端
│   ├── main.rs                   # 入口
│   ├── amas/                     # AMAS 自适应算法引擎
│   │   ├── engine.rs             #   引擎核心
│   │   ├── elo.rs                #   ELO 评分系统
│   │   ├── memory/               #   记忆模型（遗忘曲线）
│   │   ├── decision/             #   决策层
│   │   ├── word_selector.rs      #   智能选词
│   │   ├── config.rs             #   算法参数配置
│   │   ├── metrics.rs            #   性能指标
│   │   └── monitoring.rs         #   引擎监控
│   ├── routes/                   # API 路由
│   │   ├── auth.rs               #   用户认证
│   │   ├── learning.rs           #   学习流程
│   │   ├── words.rs              #   单词管理
│   │   ├── wordbooks.rs          #   词本管理
│   │   ├── wordbook_center.rs    #   词书中心
│   │   ├── records.rs            #   学习记录
│   │   ├── notifications.rs      #   通知系统
│   │   ├── realtime.rs           #   SSE 实时推送
│   │   ├── admin/                #   管理后台路由
│   │   └── ...
│   ├── workers/                  # 后台定时任务（17+）
│   │   ├── session_cleanup.rs    #   会话清理
│   │   ├── forgetting_alert.rs   #   遗忘预警
│   │   ├── daily_aggregation.rs  #   每日数据聚合
│   │   ├── delayed_reward.rs     #   延迟奖励计算
│   │   └── ...
│   ├── store/                    # 数据存储层（sled）
│   ├── middleware/               # 中间件（速率限制、请求 ID）
│   ├── services/                 # 业务服务
│   └── auth.rs                   # JWT / 密码哈希
├── frontend/                     # SolidJS 前端
│   ├── src/
│   │   ├── pages/                # 页面组件
│   │   │   ├── LearningPage.tsx  #   核心学习页面
│   │   │   ├── VocabularyPage.tsx #  词汇管理
│   │   │   ├── FlashcardPage.tsx #   闪卡复习
│   │   │   ├── StatisticsPage.tsx #  数据统计
│   │   │   ├── admin/            #   管理后台页面
│   │   │   └── ...
│   │   ├── components/           # UI 组件库
│   │   │   ├── ui/               #   通用 UI（Button, Modal, Card...）
│   │   │   ├── fatigue/          #   疲劳检测组件
│   │   │   └── layout/           #   布局组件
│   │   ├── api/                  # API 客户端
│   │   ├── stores/               # 状态管理
│   │   ├── lib/                  # 工具库
│   │   └── types/                # TypeScript 类型
│   └── tests/                    # 前端测试
├── static/                       # 静态资源 + SPA 入口
├── .env.example                  # 环境变量模板
└── Cargo.toml
```
