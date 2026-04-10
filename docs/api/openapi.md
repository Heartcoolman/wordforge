# OpenAPI 接口规范

<script setup>
import { onMounted } from 'vue'

onMounted(() => {
  const link = document.createElement('link')
  link.rel = 'stylesheet'
  link.href = 'https://unpkg.com/swagger-ui-dist@5/swagger-ui.css'
  document.head.appendChild(link)

  const script = document.createElement('script')
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
</script>

<div id="swagger-ui"></div>

<style scoped>
#swagger-ui :deep(.topbar) { display: none; }
#swagger-ui :deep(.scheme-container) { display: none; }
#swagger-ui :deep(.wrapper) { max-width: 100%; padding: 0; }
#swagger-ui :deep(.info) { margin: 24px 0; }
#swagger-ui :deep(.opblock) { margin-bottom: 12px; }
#swagger-ui :deep(.opblock .opblock-summary) { padding: 8px 16px; }
#swagger-ui :deep(.opblock-body) { padding: 16px; }
#swagger-ui :deep(table tbody tr td) { padding: 12px 16px; }
#swagger-ui :deep(.responses-inner) { padding: 16px; }
</style>
