/// 公共验证函数模块
/// 提供密码、邮箱、用户名等输入验证，供认证和用户相关路由共用。

/// 验证密码强度：至少 8 字符、最多 256 字符，需包含大小写字母和数字
pub fn validate_password(password: &str) -> Result<(), &'static str> {
    if password.len() < 8 {
        return Err("密码长度不能少于8个字符");
    }
    if password.len() > 256 {
        return Err("密码长度不能超过256个字符");
    }
    let has_upper = password.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = password.chars().any(|c| c.is_ascii_lowercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    if !has_upper || !has_lower || !has_digit {
        return Err("密码必须包含至少一个大写字母、一个小写字母和一个数字");
    }
    Ok(())
}

/// 验证邮箱格式：user@domain.tld
pub fn is_valid_email(email: &str) -> bool {
    if email.len() > 254 {
        return false;
    }
    let parts: Vec<&str> = email.splitn(2, '@').collect();
    if parts.len() != 2 {
        return false;
    }
    let (local, domain) = (parts[0], parts[1]);
    if local.is_empty() || local.len() > 64 {
        return false;
    }
    // local part 基本字符集检查：仅允许字母、数字和 . _ + - 字符
    if !local
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'+' || b == b'-')
    {
        return false;
    }
    // local part 不得以点号开头或结尾，不得包含连续点号
    if local.starts_with('.') || local.ends_with('.') || local.contains("..") {
        return false;
    }
    if domain.is_empty() || !domain.contains('.') {
        return false;
    }
    // 域名字符集检查：仅允许字母、数字、连字符和点
    if !domain
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
    {
        return false;
    }
    // 域名各部分非空，且不以连字符开头或结尾
    domain
        .split('.')
        .all(|part| !part.is_empty() && !part.starts_with('-') && !part.ends_with('-'))
}

/// 验证用户名格式：2-50 字符，只允许字母、数字、下划线、连字符和空格
pub fn validate_username(username: &str) -> Result<(), &'static str> {
    let char_count = username.chars().count();
    if char_count < 2 || char_count > 50 {
        return Err("用户名长度需在2到50个字符之间");
    }
    if !username
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == ' ')
    {
        return Err("用户名只能包含字母、数字、下划线、连字符和空格");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_password_accepted() {
        assert!(validate_password("Abc12345").is_ok());
    }

    #[test]
    fn short_password_rejected() {
        assert!(validate_password("Ab1").is_err());
    }

    #[test]
    fn no_uppercase_rejected() {
        assert!(validate_password("abcdefg1").is_err());
    }

    #[test]
    fn no_digit_rejected() {
        assert!(validate_password("Abcdefgh").is_err());
    }

    #[test]
    fn valid_email_accepted() {
        assert!(is_valid_email("user@example.com"));
    }

    #[test]
    fn email_without_dot_rejected() {
        assert!(!is_valid_email("user@example"));
    }

    #[test]
    fn email_without_at_rejected() {
        assert!(!is_valid_email("userexample.com"));
    }

    #[test]
    fn email_domain_with_special_chars_rejected() {
        assert!(!is_valid_email("user@exa mple.com"));
        assert!(!is_valid_email("user@exam!ple.com"));
        assert!(!is_valid_email("user@exam_ple.com"));
    }

    #[test]
    fn email_domain_starting_with_hyphen_rejected() {
        assert!(!is_valid_email("user@-example.com"));
    }

    #[test]
    fn email_domain_ending_with_hyphen_rejected() {
        assert!(!is_valid_email("user@example-.com"));
    }

    #[test]
    fn email_local_part_with_special_chars_rejected() {
        assert!(!is_valid_email("us er@example.com"));
        assert!(!is_valid_email("us!er@example.com"));
        assert!(!is_valid_email("us#er@example.com"));
    }

    #[test]
    fn email_local_part_with_dots_valid() {
        assert!(is_valid_email("first.last@example.com"));
    }

    #[test]
    fn email_local_part_leading_dot_rejected() {
        assert!(!is_valid_email(".user@example.com"));
    }

    #[test]
    fn email_local_part_trailing_dot_rejected() {
        assert!(!is_valid_email("user.@example.com"));
    }

    #[test]
    fn email_local_part_consecutive_dots_rejected() {
        assert!(!is_valid_email("user..name@example.com"));
    }

    #[test]
    fn email_local_part_with_plus_accepted() {
        assert!(is_valid_email("user+tag@example.com"));
    }

    #[test]
    fn email_valid_domain_with_hyphen_accepted() {
        assert!(is_valid_email("user@my-domain.com"));
    }

    #[test]
    fn valid_username_accepted() {
        assert!(validate_username("hello_world").is_ok());
    }

    #[test]
    fn short_username_rejected() {
        assert!(validate_username("a").is_err());
    }

    #[test]
    fn unicode_username_character_count_is_used() {
        assert!(validate_username("你好").is_ok());
    }

    #[test]
    fn unicode_username_over_50_characters_rejected() {
        let username = "你".repeat(51);
        assert!(validate_username(&username).is_err());
    }

    #[test]
    fn special_chars_in_username_rejected() {
        assert!(validate_username("user@name").is_err());
    }
}
