//! GeoIP 反查服务(m027)。
//!
//! 加载 MaxMind GeoLite2-Country.mmdb 数据库,提供 IP → ISO-3166-1 alpha-2
//! 国家码查询。仅在 admin/clients 链路里用,失败/未配置时返回 None,不影响主链路。
//!
//! ## 部署
//! - 文件位置:`data/GeoLite2-Country.mmdb`(与 binary 同目录或工作目录的 data/ 下)
//! - 来源:MaxMind 免费版,需注册账号获取 License Key 后下载;install.sh/release tar
//!   不自动包含(分发协议限制)。
//! - 缺失时:服务降级,country 字段全部留空,/api/* 链路不报错。

use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;

use maxminddb::geoip2;

pub type Reader = maxminddb::Reader<Vec<u8>>;

/// 试图加载 mmdb 文件;失败仅 log warn,返回 None 让主流程继续。
pub fn try_load(path: impl AsRef<Path>) -> Option<Arc<Reader>> {
    let p = path.as_ref();
    match maxminddb::Reader::open_readfile(p) {
        Ok(r) => {
            tracing::info!(path = %p.display(), "GeoIP database loaded");
            Some(Arc::new(r))
        }
        Err(e) => {
            tracing::warn!(path = %p.display(), error = %e,
                "GeoIP database missing or invalid; country field will be NULL");
            None
        }
    }
}

/// 查 IP → ISO-3166-1 alpha-2(如 "CN" / "US")。
/// IPv4 / IPv6 都支持。私网/未知 IP 返回 None。
pub fn lookup_country(reader: &Reader, ip: IpAddr) -> Option<String> {
    reader
        .lookup::<geoip2::Country>(ip)
        .ok()
        .and_then(|c| c.country)
        .and_then(|c| c.iso_code)
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_returns_none_without_panic() {
        let r = try_load("/tmp/does-not-exist.mmdb");
        assert!(r.is_none());
    }
}
