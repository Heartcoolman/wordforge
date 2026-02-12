// 防止 FOUC：在首次渲染前同步应用主题
(function() {
  var raw = localStorage.getItem('eng_theme');
  var t = null;
  try { t = JSON.parse(raw); } catch(e) { t = raw; }
  if (t === 'dark' || (!t && window.matchMedia('(prefers-color-scheme: dark)').matches)) {
    document.documentElement.classList.add('dark');
  }
})();
