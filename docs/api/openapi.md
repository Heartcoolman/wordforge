---
layout: page
---

<script setup>
import { onMounted, onUnmounted } from 'vue'

onMounted(() => {
  document.documentElement.classList.add('openapi-page')

  const link = document.createElement('link')
  link.rel = 'stylesheet'
  link.id = 'swagger-css'
  link.href = 'https://unpkg.com/swagger-ui-dist@5/swagger-ui.css'
  document.head.appendChild(link)

  const script = document.createElement('script')
  script.id = 'swagger-js'
  script.src = 'https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js'
  script.onload = () => {
    window.SwaggerUIBundle({
      url: '/wordforge/api/openapi.yaml',
      dom_id: '#swagger-ui',
      presets: [window.SwaggerUIBundle.presets.apis],
      layout: 'BaseLayout',
      defaultModelsExpandDepth: -1,
      docExpansion: 'list',
    })
  }
  document.head.appendChild(script)
})

onUnmounted(() => {
  document.documentElement.classList.remove('openapi-page')
})
</script>

<div id="swagger-ui"></div>

<style>
/* 仅在 OpenAPI 页面生效 */
html.openapi-page .VPSidebar { display: none !important; }
html.openapi-page .VPDocAside { display: none !important; }
html.openapi-page .VPDoc .container { max-width: 100% !important; }
html.openapi-page .VPDoc .content { max-width: 100% !important; padding: 0 24px !important; }
html.openapi-page .VPContent.has-sidebar { padding-left: 0 !important; }

/* Swagger UI 样式优化 */
.swagger-ui .topbar { display: none; }
.swagger-ui { font-family: inherit; }
.swagger-ui .info { margin: 24px 0; }
.swagger-ui .scheme-container { display: none; }
.swagger-ui .wrapper { max-width: 100%; padding: 0; }
.swagger-ui .opblock { margin-bottom: 12px; }
.swagger-ui .opblock .opblock-summary { padding: 8px 16px; }
.swagger-ui .opblock-body { padding: 16px; }
.swagger-ui table tbody tr td { padding: 12px 16px; }
.swagger-ui .responses-inner { padding: 16px; }
.swagger-ui .response-col_description { padding: 12px 0; }
.swagger-ui .model-box { padding: 12px; }
</style>
