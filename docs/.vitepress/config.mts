import { defineConfig } from 'vitepress'

export default defineConfig({
  lang: 'zh-CN',
  title: 'WordForge',
  description: '自适应算法驱动的智能英语学习平台',
  base: '/wordforge/',
  lastUpdated: true,

  themeConfig: {
    nav: [
      { text: '指南', link: '/guide/introduction' },
      { text: '架构', link: '/architecture/tech-stack' },
      { text: 'API', link: '/api/overview' },
      { text: '测试', link: '/testing/overview' },
    ],

    sidebar: {
      '/guide/': [
        {
          text: '指南',
          items: [
            { text: '项目简介', link: '/guide/introduction' },
            { text: '快速开始', link: '/guide/getting-started' },
            { text: '项目结构', link: '/guide/project-structure' },
            { text: '环境变量', link: '/guide/environment' },
          ],
        },
      ],
      '/architecture/': [
        {
          text: '架构设计',
          items: [
            { text: '技术栈', link: '/architecture/tech-stack' },
            { text: 'AMAS 自适应算法', link: '/architecture/amas' },
            { text: '认证机制', link: '/architecture/auth' },
            { text: '疲劳检测方案', link: '/architecture/fatigue-detection' },
            { text: '后台任务系统', link: '/architecture/workers' },
          ],
        },
      ],
      '/api/': [
        {
          text: 'API 文档',
          items: [
            { text: 'API 总览', link: '/api/overview' },
            { text: '认证 API', link: '/api/auth' },
            { text: '学习 API', link: '/api/learning' },
            { text: '单词管理 API', link: '/api/words' },
            { text: '管理后台 API', link: '/api/admin' },
          ],
        },
      ],
      '/testing/': [
        {
          text: '测试',
          items: [
            { text: '测试概览', link: '/testing/overview' },
            { text: '单元测试', link: '/testing/unit-tests' },
            { text: 'E2E 测试', link: '/testing/e2e-tests' },
            { text: '覆盖率', link: '/testing/coverage' },
          ],
        },
      ],
      '/admin/': [
        {
          text: '管理后台',
          items: [
            { text: '功能概览', link: '/admin/overview' },
          ],
        },
      ],
    },

    socialLinks: [
      { icon: 'github', link: 'https://github.com/Heartcoolman/wordforge' },
    ],

    footer: {
      message: 'WordForge — 智能英语学习平台',
    },

    outline: { label: '页面导航' },
    lastUpdated: { text: '最后更新' },
    docFooter: { prev: '上一页', next: '下一页' },
    search: { provider: 'local' },
  },
})
