import { defineConfig } from 'vitepress'

export default defineConfig({
  lang: 'zh-CN',
  title: 'WordForge',
  description: '自适应算法驱动的智能英语学习平台',
  base: '/wordforge/',
  lastUpdated: true,
  cleanUrls: true,
  ignoreDeadLinks: true,
  // 开发内部目录（设计草稿 / 调研报告），含泛型尖括号会导致 markdown-it 报未闭合 HTML 错误
  srcExclude: ['superpowers/**', 'v1-research/**'],

  themeConfig: {
    nav: [
      { text: '指南', link: '/guide/introduction' },
      { text: 'API', link: '/api-endpoints' },
      { text: 'AMAS', link: '/amas-admin-console' },
      { text: '用户', link: '/user/installation-web' },
      { text: '运维', link: '/auto-update' },
      { text: '开发者', link: '/alignment' },
    ],

    sidebar: {
      '/guide/': [
        {
          text: '入门',
          items: [
            { text: '项目简介', link: '/guide/introduction' },
            { text: '快速开始', link: '/guide/getting-started' },
            { text: '架构概览', link: '/guide/architecture' },
            { text: 'AMAS 入门', link: '/guide/amas-intro' },
            { text: '单词状态机', link: '/guide/word-states' },
          ],
        },
      ],
      '/user/': [
        {
          text: '用户文档',
          items: [
            { text: 'Web 客户端安装', link: '/user/installation-web' },
            { text: 'iOS 客户端安装', link: '/user/installation-ios' },
            { text: '常见问题', link: '/user/faq' },
            { text: '隐私与数据', link: '/user/privacy' },
          ],
        },
      ],
      '/runbook/': [
        {
          text: '运维 Runbook',
          items: [
            { text: '后端自更新', link: '/auto-update' },
            { text: '备份与恢复', link: '/runbook/backup-restore' },
            { text: '密钥轮换', link: '/runbook/key-rotation' },
            { text: '故障处理', link: '/runbook/incident-response' },
            { text: '容量规划', link: '/runbook/scaling' },
            { text: '监控对接', link: '/runbook/monitoring-setup' },
            { text: '维护模式', link: '/runbook/maintenance-mode' },
            { text: 'nginx 与 TLS', link: '/runbook/nginx-tls' },
          ],
        },
      ],
      '/': [
        {
          text: '入门',
          collapsed: false,
          items: [
            { text: '项目简介', link: '/guide/introduction' },
            { text: '快速开始', link: '/guide/getting-started' },
            { text: '架构概览', link: '/guide/architecture' },
            { text: 'AMAS 入门', link: '/guide/amas-intro' },
            { text: '单词状态机', link: '/guide/word-states' },
            { text: '更新日志', link: '/changelog' },
            { text: '发版日历', link: '/release-calendar' },
          ],
        },
        {
          text: '用户文档',
          collapsed: false,
          items: [
            { text: 'Web 客户端安装', link: '/user/installation-web' },
            { text: 'iOS 客户端安装', link: '/user/installation-ios' },
            { text: '常见问题', link: '/user/faq' },
            { text: '隐私与数据', link: '/user/privacy' },
          ],
        },
        {
          text: 'API 文档',
          collapsed: false,
          items: [
            { text: 'v1.0 客户端迁移', link: '/v1-client-migration' },
            { text: 'API 接口对接', link: '/api-endpoints' },
            { text: 'API 对接规范', link: '/api-spec' },
            { text: '客户端上传数据规范', link: '/client-upload-data' },
          ],
        },
        {
          text: 'AMAS',
          collapsed: false,
          items: [
            { text: '调参管理后台', link: '/amas-admin-console' },
            {
              text: '2026-05-15 调参记录',
              collapsed: true,
              items: [
                { text: '最终报告', link: '/amas-tuning-2026-05-15/01-final-report' },
                { text: 'FSRS-5 / DHP 研究', link: '/amas-tuning-2026-05-15/02-fsrs-dhp-research' },
                { text: 'Adapter 扩展分析', link: '/amas-tuning-2026-05-15/03-adapter-analysis' },
              ],
            },
          ],
        },
        {
          text: '运维 Runbook',
          collapsed: false,
          items: [
            { text: '后端自更新', link: '/auto-update' },
            { text: '备份与恢复', link: '/runbook/backup-restore' },
            { text: '密钥轮换', link: '/runbook/key-rotation' },
            { text: '故障处理', link: '/runbook/incident-response' },
            { text: '容量规划', link: '/runbook/scaling' },
            { text: '监控对接', link: '/runbook/monitoring-setup' },
            { text: '维护模式', link: '/runbook/maintenance-mode' },
            { text: 'nginx 与 TLS', link: '/runbook/nginx-tls' },
          ],
        },
        {
          text: '开发者参考',
          collapsed: false,
          items: [
            { text: '契约对齐审计', link: '/alignment' },
            { text: 'AMAS Schema Codegen', link: '/amas-schema-codegen' },
          ],
        },
      ],
    },

    socialLinks: [
      { icon: 'github', link: 'https://github.com/Heartcoolman/wordforge' },
    ],

    footer: { message: 'WordForge — 智能英语学习平台' },
    outline: { label: '页面导航' },
    lastUpdated: { text: '最后更新' },
    docFooter: { prev: '上一页', next: '下一页' },
    search: { provider: 'local' },
  },
})
