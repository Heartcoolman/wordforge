import { defineConfig } from 'vitepress'

export default defineConfig({
  lang: 'zh-CN',
  title: 'WordForge',
  description: '自适应算法驱动的智能英语学习平台',
  base: '/wordforge/',
  lastUpdated: true,
  cleanUrls: true,
  ignoreDeadLinks: true,

  themeConfig: {
    nav: [
      { text: 'API', link: '/api-endpoints' },
      { text: 'AMAS', link: '/amas-admin-console' },
    ],

    sidebar: [
      {
        text: 'API 文档',
        items: [
          { text: 'API 接口对接', link: '/api-endpoints' },
          { text: 'API 对接规范', link: '/api-spec' },
          { text: '客户端上传数据规范', link: '/client-upload-data' },
        ],
      },
      {
        text: 'AMAS',
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
    ],

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
