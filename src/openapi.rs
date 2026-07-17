/// M0-C2：25 个 v1-stable 端点的 OpenAPI 3.1 集中声明。
///
/// 采用集中声明式（不在各 handler 文件加注解），避免改动其他 dev 负责的 handler 文件。
/// 新增端点时：在对应 path_* 函数中追加或新建函数，再加入 `build()` 的 paths_list。
/// 新增 schema 时：在 `schemas()` 列表追加。
///
/// 导出由 `tests/openapi_export.rs` 驱动（`cargo test --test openapi_export`），
/// CI 通过 `git diff --exit-code docs/openapi.yaml` 防止规格漂移。
use utoipa::openapi::path::{
    HttpMethod, OperationBuilder, ParameterBuilder, ParameterIn, PathItemBuilder,
};
use utoipa::openapi::request_body::RequestBodyBuilder;
use utoipa::openapi::schema::{
    AllOfBuilder, ArrayBuilder, KnownFormat, ObjectBuilder, SchemaFormat, SchemaType, Type,
};
use utoipa::openapi::security::{Http, HttpAuthScheme, SecurityScheme};
use utoipa::openapi::{
    ComponentsBuilder, ContentBuilder, InfoBuilder, OpenApiBuilder, PathsBuilder, RefOr, Required,
    ResponseBuilder, ResponsesBuilder, Schema, SecurityRequirement, Server, Tag,
};

// ─── 便捷构造 ────────────────────────────────────────────────────────────────

fn string_schema() -> RefOr<Schema> {
    RefOr::T(Schema::Object(
        ObjectBuilder::new()
            .schema_type(SchemaType::Type(Type::String))
            .build(),
    ))
}

fn integer_schema() -> RefOr<Schema> {
    RefOr::T(Schema::Object(
        ObjectBuilder::new()
            .schema_type(SchemaType::Type(Type::Integer))
            .build(),
    ))
}

fn number_schema() -> RefOr<Schema> {
    RefOr::T(Schema::Object(
        ObjectBuilder::new()
            .schema_type(SchemaType::Type(Type::Number))
            .build(),
    ))
}

fn bool_schema() -> RefOr<Schema> {
    RefOr::T(Schema::Object(
        ObjectBuilder::new()
            .schema_type(SchemaType::Type(Type::Boolean))
            .build(),
    ))
}

fn uuid_schema() -> RefOr<Schema> {
    RefOr::T(Schema::Object(
        ObjectBuilder::new()
            .schema_type(SchemaType::Type(Type::String))
            .format(Some(SchemaFormat::KnownFormat(KnownFormat::Uuid)))
            .build(),
    ))
}

fn datetime_schema() -> RefOr<Schema> {
    RefOr::T(Schema::Object(
        ObjectBuilder::new()
            .schema_type(SchemaType::Type(Type::String))
            .format(Some(SchemaFormat::KnownFormat(KnownFormat::DateTime)))
            .description(Some("ISO 8601 UTC"))
            .build(),
    ))
}

fn schema_ref(name: &str) -> RefOr<Schema> {
    RefOr::Ref(utoipa::openapi::Ref::new(format!(
        "#/components/schemas/{name}"
    )))
}

fn paginated_schema(item_name: &str) -> RefOr<Schema> {
    RefOr::T(Schema::Object(
        ObjectBuilder::new()
            .property(
                "data",
                RefOr::T(Schema::Array(
                    ArrayBuilder::new().items(schema_ref(item_name)).build(),
                )),
            )
            .property("total", integer_schema())
            .property("page", integer_schema())
            .property("perPage", integer_schema())
            .property("totalPages", integer_schema())
            .required("data")
            .required("total")
            .required("page")
            .required("perPage")
            .required("totalPages")
            .build(),
    ))
}

/// `{ "success": true, "data": {} }` 包装的 200 响应。
fn ok_response(description: &str) -> utoipa::openapi::Response {
    ResponseBuilder::new()
        .description(description)
        .content(
            "application/json",
            ContentBuilder::new()
                .schema(Some(RefOr::T(Schema::AllOf(
                    AllOfBuilder::new()
                        .item(Schema::Object(
                            ObjectBuilder::new()
                                .property(
                                    "success",
                                    RefOr::T(Schema::Object(
                                        ObjectBuilder::new()
                                            .schema_type(SchemaType::Type(Type::Boolean))
                                            .build(),
                                    )),
                                )
                                .required("success")
                                .build(),
                        ))
                        .item(Schema::Object(
                            ObjectBuilder::new()
                                .property(
                                    "data",
                                    RefOr::T(Schema::Object(ObjectBuilder::new().build())),
                                )
                                .required("data")
                                .build(),
                        ))
                        .build(),
                ))))
                .build(),
        )
        .build()
}

fn unauthorized() -> utoipa::openapi::Response {
    ResponseBuilder::new()
        .description("令牌缺失或已过期 — AUTH_UNAUTHORIZED")
        .build()
}

fn not_found() -> utoipa::openapi::Response {
    ResponseBuilder::new()
        .description("资源不存在 — NOT_FOUND")
        .build()
}

fn bad_request() -> utoipa::openapi::Response {
    ResponseBuilder::new()
        .description("请求体校验失败 — VALIDATION_ERROR / INVALID_REQUEST_BODY")
        .build()
}

fn bearer_security() -> SecurityRequirement {
    SecurityRequirement::new("bearerAuth", Vec::<String>::new())
}

fn json_body(schema_name: &str) -> utoipa::openapi::request_body::RequestBody {
    RequestBodyBuilder::new()
        .content(
            "application/json",
            ContentBuilder::new()
                .schema(Some(schema_ref(schema_name)))
                .build(),
        )
        .required(Some(Required::True))
        .build()
}

fn path_param(name: &str, schema: RefOr<Schema>) -> utoipa::openapi::path::Parameter {
    ParameterBuilder::new()
        .name(name)
        .parameter_in(ParameterIn::Path)
        .required(Required::True)
        .schema(Some(schema))
        .build()
}

fn query_param(name: &str, schema: RefOr<Schema>) -> utoipa::openapi::path::Parameter {
    ParameterBuilder::new()
        .name(name)
        .parameter_in(ParameterIn::Query)
        .schema(Some(schema))
        .build()
}

// ─── 端点定义 ────────────────────────────────────────────────────────────────

fn path_auth_register() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("auth")
        .summary(Some("注册新用户"))
        .description(Some("创建账号，返回 accessToken + refresh cookie。"))
        .request_body(Some(json_body("RegisterRequest")))
        .responses(
            ResponsesBuilder::new()
                .response("201", ok_response("注册成功，含 accessToken 和用户信息"))
                .response("400", bad_request())
                .response(
                    "409",
                    ResponseBuilder::new()
                        .description("邮箱已注册 — CONFLICT")
                        .build(),
                )
                .build(),
        )
        .build();
    (
        "/auth/register".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Post, op)
            .build(),
    )
}

fn path_auth_login() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("auth")
        .summary(Some("用户登录"))
        .description(Some("返回 accessToken，写入 refresh cookie。"))
        .request_body(Some(json_body("LoginRequest")))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("登录成功，含 accessToken 和用户信息"))
                .response("400", bad_request())
                .response("401", unauthorized())
                .response(
                    "429",
                    ResponseBuilder::new()
                        .description("认证限流或账号锁定 — RATE_LIMITED / AUTH_RATE_LIMITED")
                        .build(),
                )
                .build(),
        )
        .build();
    (
        "/auth/login".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Post, op)
            .build(),
    )
}

fn path_auth_refresh() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("auth")
        .summary(Some("刷新 Access Token"))
        .description(Some("凭 refresh_token（header 或 cookie）换取新的 accessToken。旧 token 立即失效（一次性使用）。"))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("新 accessToken"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/auth/refresh".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Post, op)
            .build(),
    )
}

fn path_auth_logout() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("auth")
        .summary(Some("登出"))
        .description(Some("撤销当前 session token，清除 refresh cookie。"))
        .security(bearer_security())
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("登出成功"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/auth/logout".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Post, op)
            .build(),
    )
}

fn path_auth_forgot_password() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("auth")
        .summary(Some("请求重置密码邮件"))
        .request_body(Some(json_body("ForgotPasswordRequest")))
        .responses(
            ResponsesBuilder::new()
                .response(
                    "200",
                    ok_response("邮件已发送（无论邮箱是否存在均返回 200）"),
                )
                .build(),
        )
        .build();
    (
        "/auth/forgot-password".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Post, op)
            .build(),
    )
}

fn path_auth_reset_password() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("auth")
        .summary(Some("重置密码"))
        .request_body(Some(json_body("ResetPasswordRequest")))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("密码重置成功"))
                .response("400", bad_request())
                .build(),
        )
        .build();
    (
        "/auth/reset-password".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Post, op)
            .build(),
    )
}

fn path_user_profile() -> (String, utoipa::openapi::PathItem) {
    let get_op = OperationBuilder::new()
        .tag("user")
        .summary(Some("获取当前用户 profile"))
        .security(bearer_security())
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("用户 profile"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    let put_op = OperationBuilder::new()
        .tag("user")
        .summary(Some("更新当前用户 profile"))
        .security(bearer_security())
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("更新后的用户 profile"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/user-profile".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, get_op)
            .operation(HttpMethod::Put, put_op)
            .build(),
    )
}

fn path_words() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("words")
        .summary(Some("分页获取单词列表"))
        .security(bearer_security())
        .parameter(query_param("page", integer_schema()))
        .parameter(query_param("perPage", integer_schema()))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("分页单词列表"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/words".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, op)
            .build(),
    )
}

fn path_words_id() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("words")
        .summary(Some("获取单个单词详情"))
        .security(bearer_security())
        .parameter(path_param("wordId", uuid_schema()))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("单词详情"))
                .response("401", unauthorized())
                .response("404", not_found())
                .build(),
        )
        .build();
    (
        "/words/{wordId}".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, op)
            .build(),
    )
}

fn path_records() -> (String, utoipa::openapi::PathItem) {
    let post_op = OperationBuilder::new()
        .tag("learning")
        .summary(Some("提交学习记录"))
        .description(Some(
            "记录一次单词作答。`recordType` 缺省落库 `all`；`selfRating` 可选。`clientRecordId` \
             强烈建议携带并在重试时复用同一个值——服务端以此为幂等键，缺省时重试/离线重放会\
             被当成两次独立事件二次计入 AMAS 状态。",
        ))
        .security(bearer_security())
        .request_body(Some(json_body("CreateRecordRequest")))
        .responses(
            ResponsesBuilder::new()
                .response("201", ok_response("已创建学习记录"))
                .response(
                    "200",
                    ok_response("命中 clientRecordId 幂等去重，返回原记录（`duplicate: true`）"),
                )
                .response(
                    "202",
                    ok_response(
                        "RECORDS_OUTBOX_ASYNC 开启时的异步落库确认：`{accepted, async, clientRecordId}`，\
                         不含 record/amasResult 字段，响应形状与 200/201 不同",
                    ),
                )
                .response("400", bad_request())
                .response("401", unauthorized())
                .build(),
        )
        .build();
    let get_op = OperationBuilder::new()
        .tag("learning")
        .summary(Some("获取学习记录列表"))
        .security(bearer_security())
        .parameter(query_param("page", integer_schema()))
        .parameter(query_param("perPage", integer_schema()))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("分页学习记录"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/records".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Post, post_op)
            .operation(HttpMethod::Get, get_op)
            .build(),
    )
}

fn path_learning_session() -> (String, utoipa::openapi::PathItem) {
    // 契约修正：真实路由是单数 /learning/session（routes/learning/mod.rs:20），复数
    // /learning/sessions 上挂的是 GET 会话列表（session::list_sessions），不是本操作。
    // create_or_resume_session 无论"恢复既有会话"还是"新建会话"分支都走 ok()，从不返回
    // 201——spec 之前文档的路径和状态码都与真实路由不符。
    let op = OperationBuilder::new()
        .tag("learning")
        .summary(Some("开始或恢复学习会话"))
        .security(bearer_security())
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("会话（新建或恢复既有 active 会话）"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/learning/session".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Post, op)
            .build(),
    )
}

fn path_word_states_id() -> (String, utoipa::openapi::PathItem) {
    // 契约修正：真实路由（routes/word_states.rs:56-64）在这个路径上只挂 GET，从未注册过
    // PUT——之前文档的 PUT 操作永不可达。真正的写入面是三个独立路径，见下面三个函数。
    let get_op = OperationBuilder::new()
        .tag("word-states")
        .summary(Some("获取单词学习状态"))
        .description(Some(
            "`state` 枚举值为 lowercase：`new/learning/reviewing/mastered/forgotten`\n\
             （v0.6.0-beta.4 P3#7 起；与 MasteryLevel SCREAMING_SNAKE_CASE 是不同枚举）",
        ))
        .security(bearer_security())
        .parameter(path_param("wordId", uuid_schema()))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("单词学习状态（WordLearningState）"))
                .response("401", unauthorized())
                .response("404", not_found())
                .build(),
        )
        .build();
    (
        "/word-states/{wordId}".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, get_op)
            .build(),
    )
}

fn path_word_states_mark_mastered() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("word-states")
        .summary(Some("标记单词为已掌握"))
        .description(Some("mastery_level 置 1.0，state 置 mastered，清除复习排程。"))
        .security(bearer_security())
        .parameter(path_param("wordId", uuid_schema()))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("更新后的单词学习状态"))
                .response("401", unauthorized())
                .response("404", not_found())
                .build(),
        )
        .build();
    (
        "/word-states/{wordId}/mark-mastered".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Post, op)
            .build(),
    )
}

fn path_word_states_reset() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("word-states")
        .summary(Some("重置单词学习状态"))
        .description(Some("state 置 new，mastery_level/streak/attempts 全部清零，视为从未学过。"))
        .security(bearer_security())
        .parameter(path_param("wordId", uuid_schema()))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("重置后的单词学习状态"))
                .response("401", unauthorized())
                .response("404", not_found())
                .build(),
        )
        .build();
    (
        "/word-states/{wordId}/reset".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Post, op)
            .build(),
    )
}

fn path_word_states_batch_update() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("word-states")
        .summary(Some("批量更新单词学习状态"))
        .description(Some(
            "每项 state（lowercase：new/learning/reviewing/mastered/forgotten）/masteryLevel \
             均可选；只传 state 未传 masteryLevel 时，mastered/new 会强制对齐 masteryLevel 为 \
             1.0/0.0。数量上限见 config.rs limits.max_batch_size。",
        ))
        .security(bearer_security())
        .request_body(Some(json_body("BatchUpdateRequest")))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("`{updated: number}`，实际更新的条数"))
                .response("400", bad_request())
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/word-states/batch-update".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Post, op)
            .build(),
    )
}

fn path_word_favorites() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("favorites")
        .summary(Some("获取收藏单词列表"))
        .description(Some(
            "v0.6.0-beta.4 P3#5 起返回分页结构，列表在 `data.data`（非 `data` 直接为数组）。",
        ))
        .security(bearer_security())
        .parameter(query_param("page", integer_schema()))
        .parameter(query_param("perPage", integer_schema()))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("分页收藏列表（data.data）"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/word-favorites".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, op)
            .build(),
    )
}

fn path_word_favorites_id() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("favorites")
        .summary(Some("切换单词收藏状态"))
        .security(bearer_security())
        .parameter(path_param("wordId", uuid_schema()))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("收藏结果，含 `favorited: bool`"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/word-favorites/{wordId}".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Post, op)
            .build(),
    )
}

fn path_word_notes() -> (String, utoipa::openapi::PathItem) {
    let get_op = OperationBuilder::new()
        .tag("notes")
        .summary(Some("获取单词笔记列表"))
        .security(bearer_security())
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("分页笔记列表"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    let post_op = OperationBuilder::new()
        .tag("notes")
        .summary(Some("创建单词笔记"))
        .security(bearer_security())
        .responses(
            ResponsesBuilder::new()
                .response("201", ok_response("已创建笔记"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/word-notes".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, get_op)
            .operation(HttpMethod::Post, post_op)
            .build(),
    )
}

fn path_wordbooks() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("wordbooks")
        .summary(Some("获取词书列表"))
        .security(bearer_security())
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("词书列表"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/wordbooks".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, op)
            .build(),
    )
}

fn path_study_config() -> (String, utoipa::openapi::PathItem) {
    let get_op = OperationBuilder::new()
        .tag("config")
        .summary(Some("获取学习配置"))
        .security(bearer_security())
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("当前学习配置"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    let put_op = OperationBuilder::new()
        .tag("config")
        .summary(Some("更新学习配置"))
        .security(bearer_security())
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("更新后的学习配置"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/study-config".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, get_op)
            .operation(HttpMethod::Put, put_op)
            .build(),
    )
}

fn path_analytics_dashboard() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("analytics")
        .summary(Some("学习统计 dashboard"))
        .description(Some(
            "支持 `?range=day|week|month`，时区固定为 Asia/Shanghai。",
        ))
        .security(bearer_security())
        .parameter(query_param(
            "range",
            RefOr::T(Schema::Object(
                ObjectBuilder::new()
                    .schema_type(SchemaType::Type(Type::String))
                    .enum_values(Some(["day", "week", "month"]))
                    .build(),
            )),
        ))
        .responses(
            ResponsesBuilder::new()
                .response(
                    "200",
                    ok_response("Dashboard 统计数据，含 summary / daily 数组"),
                )
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/analytics/dashboard".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, op)
            .build(),
    )
}

fn path_realtime_sse() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("realtime")
        .summary(Some("SSE 实时事件流"))
        .description(Some(
            "Server-Sent Events 持久连接。\n\n\
             事件类型（`event:` 字段）：\n\
             - `maintenance`、`telemetry_request`、`banned`、`unbanned`、`data_corrupted` — v1 stable\n\
             - `update_available` — v1 stable（面向所有用户，由 broadcast_update 发出）\n\
             - `release_available`、`update_progress` — v1 stable，管理员专属\n\
             - `new_llm_suggestion` — v1beta，管理员专属\n\
             - `probe_request`、`probe_confirm` — v0/internal\n\n\
             Keep-alive 行 `: keepalive` 每 15 秒发送一次。",
        ))
        .security(bearer_security())
        .responses(
            ResponsesBuilder::new()
                .response(
                    "200",
                    ResponseBuilder::new()
                        .description("text/event-stream 持久连接")
                        .content(
                            "text/event-stream",
                            ContentBuilder::new()
                                .schema(Some(string_schema()))
                                .build(),
                        )
                        .build(),
                )
                .response("401", unauthorized())
                .response(
                    "429",
                    ResponseBuilder::new()
                        .description("连接数超限 — RATE_LIMITED")
                        .build(),
                )
                .build(),
        )
        .build();
    (
        "/realtime/events".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, op)
            .build(),
    )
}

fn path_feedback() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("misc")
        .summary(Some("提交用户反馈"))
        .security(bearer_security())
        .responses(
            ResponsesBuilder::new()
                .response("201", ok_response("已提交反馈"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/feedback".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Post, op)
            .build(),
    )
}

fn path_status() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("misc")
        .summary(Some("版本探测"))
        .description(Some(
            "返回服务器版本号；客户端用于最低版本门检查。无需认证。",
        ))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("服务器版本信息"))
                .build(),
        )
        .build();
    (
        "/status".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, op)
            .build(),
    )
}

fn path_health() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("misc")
        .summary(Some("健康检查"))
        .description(Some(
            "LB / k8s 探针。不使用统一 success/data 包装，直接返回健康数据。",
        ))
        .responses(
            ResponsesBuilder::new()
                .response(
                    "200",
                    ResponseBuilder::new().description("服务正常").build(),
                )
                .response(
                    "503",
                    ResponseBuilder::new()
                        .description("服务异常或维护中")
                        .build(),
                )
                .build(),
        )
        .build();
    (
        "/health".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, op)
            .build(),
    )
}

// ─── Schema 组件 ─────────────────────────────────────────────────────────────

fn schemas() -> Vec<(String, RefOr<Schema>)> {
    vec![
        (
            "RegisterRequest".to_string(),
            RefOr::T(Schema::Object(
                ObjectBuilder::new()
                    .property("email", string_schema())
                    .property("username", string_schema())
                    .property("password", string_schema())
                    .required("email")
                    .required("username")
                    .required("password")
                    .build(),
            )),
        ),
        (
            "BatchUpdateRequest".to_string(),
            RefOr::T(Schema::Object(
                ObjectBuilder::new()
                    .property(
                        "updates",
                        RefOr::T(Schema::Array(
                            ArrayBuilder::new()
                                .items(RefOr::T(Schema::Object(
                                    ObjectBuilder::new()
                                        .property("wordId", string_schema())
                                        .property("state", string_schema())
                                        .property("masteryLevel", number_schema())
                                        .required("wordId")
                                        .build(),
                                )))
                                .build(),
                        )),
                    )
                    .required("updates")
                    .build(),
            )),
        ),
        (
            "LoginRequest".to_string(),
            RefOr::T(Schema::Object(
                ObjectBuilder::new()
                    .property("email", string_schema())
                    .property("password", string_schema())
                    .required("email")
                    .required("password")
                    .build(),
            )),
        ),
        (
            "ForgotPasswordRequest".to_string(),
            RefOr::T(Schema::Object(
                ObjectBuilder::new()
                    .property("email", string_schema())
                    .required("email")
                    .build(),
            )),
        ),
        (
            "ResetPasswordRequest".to_string(),
            RefOr::T(Schema::Object(
                ObjectBuilder::new()
                    .property("token", string_schema())
                    .property("newPassword", string_schema())
                    .required("token")
                    .required("newPassword")
                    .build(),
            )),
        ),
        (
            "CreateRecordRequest".to_string(),
            RefOr::T(Schema::Object(
                ObjectBuilder::new()
                    .property("wordId", uuid_schema())
                    .property("isCorrect", bool_schema())
                    .property("responseTimeMs", integer_schema())
                    .property("sessionId", uuid_schema())
                    .property(
                        "recordType",
                        RefOr::T(Schema::Object(
                            ObjectBuilder::new()
                                .schema_type(SchemaType::Type(Type::String))
                                .enum_values(Some(["learning", "review", "all"]))
                                .build(),
                        )),
                    )
                    .property("selfRating", integer_schema())
                    // 幂等键，可选但强烈建议携带——见 path_records() 的 description。缺省时若
                    // 请求被重试/离线重放，服务端无法识别为同一事件，会二次创建记录并二次计入
                    // AMAS mastery/EVM/trust 状态（本轮 storage-integrity 高危修复的直接成因）。
                    .property("clientRecordId", string_schema())
                    .required("wordId")
                    .required("isCorrect")
                    .required("responseTimeMs")
                    .build(),
            )),
        ),
        (
            "WordLearningState".to_string(),
            RefOr::T(Schema::Object(
                ObjectBuilder::new()
                    .property("userId", uuid_schema())
                    .property("wordId", uuid_schema())
                    .property(
                        "state",
                        RefOr::T(Schema::Object(
                            ObjectBuilder::new()
                                .schema_type(SchemaType::Type(Type::String))
                                .enum_values(Some([
                                    "new",
                                    "learning",
                                    "reviewing",
                                    "mastered",
                                    "forgotten",
                                ]))
                                .description(Some(
                                    "lowercase；v0.6.0-beta.4 P3#7 起。\
                                     与 MasteryLevel（SCREAMING_SNAKE_CASE）是不同枚举，不可混用",
                                ))
                                .build(),
                        )),
                    )
                    .property("masteryLevel", number_schema())
                    .property("nextReviewDate", datetime_schema())
                    .property("halfLife", number_schema())
                    .property("correctStreak", integer_schema())
                    .property("totalAttempts", integer_schema())
                    .property("updatedAt", datetime_schema())
                    .build(),
            )),
        ),
        (
            "PaginatedFavorites".to_string(),
            paginated_schema("FavoriteItem"),
        ),
        (
            "FavoriteItem".to_string(),
            RefOr::T(Schema::Object(
                ObjectBuilder::new()
                    .property("wordId", uuid_schema())
                    .property("favorited", bool_schema())
                    .property("createdAt", datetime_schema())
                    .build(),
            )),
        ),
    ]
}

// ─── 主入口 ──────────────────────────────────────────────────────────────────

/// 构建包含 v1-stable 端点的 OpenAPI 3.1 文档。
///
/// 路径前缀 `/api` 由 servers[0].url 表达，路径本身不含 `/api`。
/// 端点按路径分组（同路径多方法合并为一个 PathItem），共约 21 个唯一路径、25 个操作。
pub fn build() -> utoipa::openapi::OpenApi {
    let paths_list: Vec<(String, utoipa::openapi::PathItem)> = vec![
        // auth（6 个操作，6 个路径）
        path_auth_register(),
        path_auth_login(),
        path_auth_refresh(),
        path_auth_logout(),
        path_auth_forgot_password(),
        path_auth_reset_password(),
        // user（2 个操作，1 个路径 GET+PUT）
        path_user_profile(),
        // words（2 个操作，2 个路径）
        path_words(),
        path_words_id(),
        // learning（3 个操作，2 个路径）
        path_records(),
        path_learning_session(),
        // word-states（GET 单个 + 3 个独立写操作，无 PUT——契约修正见函数注释）
        path_word_states_id(),
        path_word_states_mark_mastered(),
        path_word_states_reset(),
        path_word_states_batch_update(),
        // favorites（2 个操作，2 个路径）
        path_word_favorites(),
        path_word_favorites_id(),
        // notes（2 个操作，1 个路径 GET+POST）
        path_word_notes(),
        // wordbooks（1 个操作，1 个路径）
        path_wordbooks(),
        // config（2 个操作，1 个路径 GET+PUT）
        path_study_config(),
        // analytics（1 个操作，1 个路径）
        path_analytics_dashboard(),
        // realtime SSE（1 个操作，1 个路径）
        path_realtime_sse(),
        // misc（3 个操作，3 个路径）
        path_feedback(),
        path_status(),
        path_health(),
    ];

    let mut paths_builder = PathsBuilder::new();
    for (path, item) in paths_list {
        paths_builder = paths_builder.path(path, item);
    }

    let mut components_builder = ComponentsBuilder::new().security_scheme(
        "bearerAuth",
        SecurityScheme::Http(
            Http::builder()
                .scheme(HttpAuthScheme::Bearer)
                .bearer_format("JWT")
                .build(),
        ),
    );
    for (name, schema) in schemas() {
        components_builder = components_builder.schema(name, schema);
    }

    OpenApiBuilder::new()
        .info(
            InfoBuilder::new()
                .title("WordForge API")
                // W4-1：从 CARGO_PKG_VERSION 取值（确定性、无 v 前缀），避免跨 v1.1.x 漂移。
                // 勿用 GIT_VERSION——CI shallow checkout 无 tag 会 fallback 导致 drift 守卫 flaky。
                .version(env!("CARGO_PKG_VERSION"))
                .description(Some(
                    "WordForge 后端 API — v1-stable 端点。\n\n\
                     路径基础路径 `/api`（见 servers[0].url）。\n\
                     本规格由 `src/openapi.rs` 自动生成，通过 `cargo test --test openapi_export` 导出，\
                     CI 以 `git diff --exit-code docs/openapi.yaml` 防止规格漂移（drift）。",
                ))
                .build(),
        )
        .servers(Some(vec![Server::new("/api")]))
        .tags(Some(vec![
            Tag::builder().name("auth").description(Some("认证与授权")).build(),
            Tag::builder().name("user").description(Some("用户 profile")).build(),
            Tag::builder().name("words").description(Some("单词资源")).build(),
            Tag::builder().name("learning").description(Some("学习记录与会话")).build(),
            Tag::builder().name("word-states").description(Some("单词学习状态")).build(),
            Tag::builder().name("favorites").description(Some("收藏管理")).build(),
            Tag::builder().name("notes").description(Some("单词笔记")).build(),
            Tag::builder().name("wordbooks").description(Some("词书")).build(),
            Tag::builder().name("config").description(Some("学习配置")).build(),
            Tag::builder().name("analytics").description(Some("统计分析")).build(),
            Tag::builder().name("realtime").description(Some("SSE 实时推送")).build(),
            Tag::builder().name("misc").description(Some("反馈 / 状态 / 健康检查")).build(),
        ]))
        .paths(paths_builder.build())
        .components(Some(components_builder.build()))
        .build()
}
