/// M0-C4：Deprecation / Sunset response header 中间件（RFC 8594）。
///
/// 用法：在需要标记弃用的路由器上添加此中间件层。
///
/// ```ignore
/// v1::router().layer(axum::middleware::from_fn(
///     deprecation::deprecation_headers(
///         "Sat, 01 Jan 2026 00:00:00 GMT",
///         "Sat, 01 Jul 2026 00:00:00 GMT",
///         Some("https://heartcoolman.github.io/wordforge/api-endpoints#v1-deprecation"),
///     ),
/// ))
/// ```
///
/// 响应头格式（RFC 8594）：
/// - `Deprecation: <HTTP-date>`  — 端点开始弃用的日期
/// - `Sunset: <HTTP-date>`       — 端点预计停止服务的日期
/// - `Link: <url>; rel="sunset"` — 可选的迁移文档链接
use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;

/// 构造 Deprecation/Sunset header 注入中间件函数。
///
/// - `deprecation_date`：RFC 1123 HTTP 日期字符串，如 `"Sat, 01 Jan 2026 00:00:00 GMT"`
/// - `sunset_date`：同上，端点预计停止服务日期
/// - `link_url`：可选迁移文档 URL，注入为 `Link: <url>; rel="sunset"`
pub fn make_deprecation_layer(
    deprecation_date: &'static str,
    sunset_date: &'static str,
    link_url: Option<&'static str>,
) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>
       + Clone
       + Send
       + 'static {
    move |req: Request, next: Next| {
        Box::pin(async move {
            let mut response = next.run(req).await;
            let headers = response.headers_mut();

            if let Ok(v) = HeaderValue::from_str(deprecation_date) {
                headers.insert("Deprecation", v);
            }
            if let Ok(v) = HeaderValue::from_str(sunset_date) {
                headers.insert("Sunset", v);
            }
            if let Some(url) = link_url {
                let link_val = format!("<{url}>; rel=\"sunset\"");
                if let Ok(v) = HeaderValue::from_str(&link_val) {
                    headers.insert("Link", v);
                }
            }

            response
        })
    }
}

/// `/api/v1/*` 专用弃用声明常量。
///
/// - 弃用日期：v0.6.0-beta.4 发布后即标弃用（2026-05-21）
/// - Sunset 日期：v2.0.0 计划切换期（2027-01-01，届时 major bump）
/// - 迁移文档：已部署文档站 api-endpoints §18（VitePress base=/wordforge/ + cleanUrls，
///   §18 标题挂稳定锚点 {#v1-deprecation}）。常量须为可见 ASCII（HeaderValue 约束），故用锚点而非中文标题。
pub const V1_DEPRECATION_DATE: &str = "Wed, 21 May 2026 00:00:00 GMT";
pub const V1_SUNSET_DATE: &str = "Fri, 01 Jan 2027 00:00:00 GMT";
pub const V1_LINK_URL: &str =
    "https://heartcoolman.github.io/wordforge/api-endpoints#v1-deprecation";
