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

<style>
.swagger-ui .topbar { display: none; }
.swagger-ui { font-family: inherit; }
.swagger-ui .info { margin: 20px 0; }
.swagger-ui .scheme-container { display: none; }
</style>
