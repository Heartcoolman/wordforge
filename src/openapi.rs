/// M0-C2：v1-stable 端点的 OpenAPI 3.1 集中声明（端点随版本演进，不在此处维护具体计数）。
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

/// query_param 的 required 变体：handler 侧 serde 无 default 的必填 query 用它，
/// 缺失即 400，spec 与真实校验行为对齐。
fn query_param_required(name: &str, schema: RefOr<Schema>) -> utoipa::openapi::path::Parameter {
    ParameterBuilder::new()
        .name(name)
        .parameter_in(ParameterIn::Query)
        .required(Required::True)
        .schema(Some(schema))
        .build()
}

/// 必填请求头参数（如 X-Device-Id）。
fn header_param_required(name: &str, schema: RefOr<Schema>) -> utoipa::openapi::path::Parameter {
    ParameterBuilder::new()
        .name(name)
        .parameter_in(ParameterIn::Header)
        .required(Required::True)
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
                    // 202 是裸 JSON 对象（handler 直接 Json(json!({accepted, async,
                    // clientRecordId}))），不走 {success,data} 信封，勿复用 ok_response。
                    ResponseBuilder::new()
                        .description(
                            "v1.3.0 起默认（RECORDS_OUTBOX_ASYNC=true）的异步落库确认。裸 JSON 对象（非 \
                             {success,data} 信封）：`{accepted: true, async: true, \
                             clientRecordId: string}`（缺省时服务端生成），不含 record/amasResult \
                             字段，响应形状与 200/201 不同。设 RECORDS_OUTBOX_ASYNC=false 回退同步老路。",
                        )
                        .content(
                            "application/json",
                            ContentBuilder::new()
                                .schema(Some(RefOr::T(Schema::Object(
                                    ObjectBuilder::new()
                                        .property("accepted", bool_schema())
                                        .property("async", bool_schema())
                                        .property("clientRecordId", string_schema())
                                        .required("accepted")
                                        .required("async")
                                        .required("clientRecordId")
                                        .build(),
                                ))))
                                .build(),
                        )
                        .build(),
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
    // /learning/sessions 上挂的是 GET 会话列表（见 path_learning_sessions_list）。
    // create_or_resume_session 无论"恢复既有会话"还是"新建会话"分支都走 ok()，从不返回
    // 201——spec 之前文档的路径和状态码都与真实路由不符。
    let op = OperationBuilder::new()
        .tag("learning")
        .summary(Some("开始或恢复学习会话"))
        .security(bearer_security())
        // 可选 body：{targetMasteryCount}；缺省取学习配置 daily_mastery_target。
        // 恢复既有 active 会话时该值不生效（沿用原会话目标）。
        .request_body(Some(
            RequestBodyBuilder::new()
                .content(
                    "application/json",
                    ContentBuilder::new()
                        .schema(Some(RefOr::T(Schema::Object(
                            ObjectBuilder::new()
                                .property("targetMasteryCount", integer_schema())
                                .build(),
                        ))))
                        .build(),
                )
                .required(Some(Required::False))
                .build(),
        ))
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

fn path_learning_sessions_list() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("learning")
        .summary(Some("获取学习会话列表"))
        .security(bearer_security())
        .parameter(query_param("page", integer_schema()))
        .parameter(query_param("perPage", integer_schema()))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("分页学习会话列表"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/learning/sessions".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, op)
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

fn path_word_states_batch_query() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("word-states")
        .summary(Some("批量查询单词学习状态"))
        .description(Some(
            "读侧批量查询（区别于 /word-states/batch-update 写侧）。数量上限见 config.rs \
             limits.max_batch_size，超限 400 BATCH_TOO_LARGE。",
        ))
        .security(bearer_security())
        .request_body(Some(json_body("BatchQueryRequest")))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("WordLearningState 数组（每项附 bookmarked）"))
                .response("400", bad_request())
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/word-states/batch".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Post, op)
            .build(),
    )
}

fn path_word_states_due_list() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("word-states")
        .summary(Some("获取当前已到期的复习单词列表"))
        .description(Some(
            "「已到期」含历史逾期（next_review_date <= now，非仅今日窗口）。\
             limit 缺省 50，钳到 1..=200；到期总数超过 limit 时仅返回一页，\
             真实总数看 /word-states/stats/overview 的 dueCount。",
        ))
        .security(bearer_security())
        .parameter(query_param("limit", integer_schema()))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("到期 WordLearningState 数组（每项附 bookmarked）"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/word-states/due/list".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, op)
            .build(),
    )
}

fn path_word_states_stats_overview() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("word-states")
        .summary(Some("单词学习状态统计总览"))
        .description(Some(
            "category 缺省 all（可选 learning/review，非法值 400 INVALID_CATEGORY）。\
             返回各状态计数 + dueCount / dueReviewEstimatedMinutes。\
             dueCount 可空（序列化时 None 直接省略字段）：当前已到期总数（含历史逾期，\
             未分页真实总量）——注意它恒为全局口径，不随 category 过滤变化。",
        ))
        .security(bearer_security())
        .parameter(query_param(
            "category",
            RefOr::T(Schema::Object(
                ObjectBuilder::new()
                    .schema_type(SchemaType::Type(Type::String))
                    .enum_values(Some(["all", "learning", "review"]))
                    .build(),
            )),
        ))
        .responses(
            ResponsesBuilder::new()
                .response(
                    "200",
                    ok_response(
                        "统计总览：`{newCount, learning, reviewing, mastered, forgotten, \
                         dueReviewEstimatedMinutes?, dueCount?}`（后两者可空省略）",
                    ),
                )
                .response("400", bad_request())
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/word-states/stats/overview".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, op)
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

fn path_word_favorites_status() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("favorites")
        .summary(Some("批量查询单词收藏状态"))
        .description(Some("数量上限见 config.rs limits.max_batch_size，超限 400 WORD_FAVORITES_TOO_MANY_IDS。"))
        .security(bearer_security())
        .parameter(query_param_required("wordIds", string_schema()))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("每项 `{wordId, favorited, createdAt}`，未收藏则 createdAt 为 null"))
                .response("400", bad_request())
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/word-favorites/status".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, op)
            .build(),
    )
}

fn path_word_favorites_id() -> (String, utoipa::openapi::PathItem) {
    let post_op = OperationBuilder::new()
        .tag("favorites")
        .summary(Some("收藏单词"))
        .description(Some("upsert 语义，非切换——重复调用恒返回 favorited:true。取消收藏须调用同路径 DELETE。"))
        .security(bearer_security())
        .parameter(path_param("wordId", uuid_schema()))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("收藏结果，`favorited` 恒为 true"))
                .response("401", unauthorized())
                .response("404", not_found())
                .build(),
        )
        .build();
    let delete_op = OperationBuilder::new()
        .tag("favorites")
        .summary(Some("取消收藏单词"))
        .security(bearer_security())
        .parameter(path_param("wordId", uuid_schema()))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("`{wordId, favorited: false, deleted: bool}`，deleted 反映本次调用前是否确实存在收藏记录"))
                .response("401", unauthorized())
                .build(),
        )
        .build();
    (
        "/word-favorites/{wordId}".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Post, post_op)
            .operation(HttpMethod::Delete, delete_op)
            .build(),
    )
}

// 客户端公开访问的资源包端点（v1.1-P0.3，全部匿名）。信封口径不统一：仅 manifest/public-key
// 是 CDN 友好的裸 JSON（供边缘缓存/客户端直接解析）；列表端点走 ok() 统一 {success,data} 信封。

fn path_resource_packs_list() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("resource-packs")
        .summary(Some("列出所有资源包元数据"))
        .responses(
            ResponsesBuilder::new()
                // 真实 handler 用 ok(packs) 包 {success,data} 信封（routes/resource_packs.rs
                // list_packs），与 manifest/public-key 的裸 JSON 不同，勿混淆。
                .response("200", ok_response("资源包元数据数组（{success, data} 信封，data 为数组）"))
                .build(),
        )
        .build();
    (
        "/resource-packs".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, op)
            .build(),
    )
}

fn path_resource_packs_public_key() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("resource-packs")
        .summary(Some("获取资源包签名 Ed25519 公钥"))
        .description(Some("客户端硬编码公钥才是正式验签信任锚，本端点仅供 verify SDK 自检。`{publicKey, algorithm}` 裸 JSON。"))
        .responses(
            ResponsesBuilder::new()
                .response("200", ResponseBuilder::new().description("`{publicKey: base64 string, algorithm: \"ed25519\"}`").build())
                .response("503", ResponseBuilder::new().description("签名器未初始化 — RESOURCE_PACK_SIGNER_UNAVAILABLE").build())
                .build(),
        )
        .build();
    (
        "/resource-packs/public-key".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, op)
            .build(),
    )
}

fn path_resource_packs_manifest() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("resource-packs")
        .summary(Some("获取指定 pack 当前 channel 激活版本的 manifest"))
        .description(Some(
            "appVersion/locale 必填，channel 缺省 stable。支持 If-None-Match，命中返回 304。\
             裸 JSON（非信封），字段名以 ResourcePackManifest 序列化输出为准（注意是 downloadURL，\
             URL 三字母全大写）。",
        ))
        .parameter(path_param("packId", string_schema()))
        .parameter(query_param_required("appVersion", string_schema()))
        .parameter(query_param_required("locale", string_schema()))
        .parameter(query_param("channel", string_schema()))
        .responses(
            ResponsesBuilder::new()
                .response("200", ResponseBuilder::new().description("`{packId, version, downloadURL, sha256, sizeBytes, minAppVersion, channel, signature, signatureAlgorithm}`").build())
                .response("304", ResponseBuilder::new().description("If-None-Match 命中，无 body").build())
                .response("400", bad_request())
                .response("404", ResponseBuilder::new().description("资源包/该 channel 无激活版本 — RESOURCE_PACK_NOT_FOUND").build())
                .response("409", ResponseBuilder::new().description("appVersion 低于 minAppVersion — RESOURCE_PACK_APP_VERSION_TOO_LOW").build())
                .build(),
        )
        .build();
    (
        "/resource-packs/{packId}/manifest".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Get, op)
            .build(),
    )
}

fn path_telemetry_resource_pack_install() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("telemetry")
        .summary(Some("上报资源包安装结果"))
        .description(Some(
            "outcome 五态（m070 起）：installed / verify_failed / rollback / download_failed / \
             apply_failed。须带 X-Device-Id 头，设备须已通过正常登录注册，否则 403。",
        ))
        .security(bearer_security())
        .parameter(header_param_required("X-Device-Id", string_schema()))
        .request_body(Some(json_body("ResourcePackInstallReport")))
        .responses(
            ResponsesBuilder::new()
                .response("200", ok_response("`{received: true}`"))
                .response(
                    "400",
                    ResponseBuilder::new()
                        .description(
                            "请求体校验失败 — VALIDATION_ERROR / INVALID_REQUEST_BODY；\
                             缺 X-Device-Id 头 — MISSING_DEVICE_ID；\
                             X-Device-Id 格式非法 — INVALID_DEVICE_ID",
                        )
                        .build(),
                )
                .response("401", unauthorized())
                .response("403", ResponseBuilder::new().description("设备未注册 / 设备归属与当前账号不符 — DEVICE_NOT_REGISTERED / DEVICE_OWNERSHIP_MISMATCH").build())
                .build(),
        )
        .build();
    (
        "/telemetry/resource-pack-install".to_string(),
        PathItemBuilder::new()
            .operation(HttpMethod::Post, op)
            .build(),
    )
}

fn path_telemetry_app_events() -> (String, utoipa::openapi::PathItem) {
    let op = OperationBuilder::new()
        .tag("telemetry")
        .summary(Some("批量上报客户端埋点命名事件（behavior/error/perf）"))
        .description(Some(
            "m073 埋点事件流。批量 ≤50 条/请求；逐条校验部分成功（errors[] 带 index/code），\
             不整批 400。name 须匹配 ^[a-z0-9_]{1,64}$；props 为 ≤16 键标量对象（字符串 ≤128 \
             字符、序列化 ≤2KB）。clientTsMs 钳制 [now-7d, now+5min]（离线补传窗，宽于 learning \
             事件的 30min 系有意为之）。(deviceId, clientEventId) 跨请求幂等去重。behavior/perf \
             受 probe_sampling_config 的 app_behavior/app_perf 行采样，error 恒不采样。\
             须带 X-Device-Id / X-Device-Platform / X-App-Version 头，设备须已注册。",
        ))
        .security(bearer_security())
        .parameter(header_param_required("X-Device-Id", string_schema()))
        .parameter(header_param_required("X-Device-Platform", string_schema()))
        .parameter(header_param_required("X-App-Version", string_schema()))
        .request_body(Some(json_body("AppEventsRequest")))
        .responses(
            ResponsesBuilder::new()
                .response(
                    "200",
                    ok_response(
                        "`{accepted, duplicates, sampledOut, failed, errors: [{index, \
                         clientEventId, code, message}]}`；限流命中软丢弃 `{accepted: 0, \
                         throttled: true}`。逐条错误码：APP_EVENT_INVALID_ID / \
                         APP_EVENT_DUPLICATE_ID / APP_EVENT_INVALID_NAME / \
                         APP_EVENT_INVALID_CATEGORY / APP_EVENT_INVALID_TS / \
                         APP_EVENT_INVALID_PROPS",
                    ),
                )
                .response(
                    "400",
                    ResponseBuilder::new()
                        .description(
                            "缺头 — MISSING_DEVICE_ID / MISSING_OS / MISSING_APP_VERSION；\
                             X-Device-Id 格式非法 — INVALID_DEVICE_ID；\
                             批量超限(>50) — APP_EVENTS_TOO_LARGE",
                        )
                        .build(),
                )
                .response("401", unauthorized())
                .response("403", ResponseBuilder::new().description("设备未注册 / 设备归属与当前账号不符 — DEVICE_NOT_REGISTERED / DEVICE_OWNERSHIP_MISMATCH").build())
                .build(),
        )
        .build();
    (
        "/telemetry/app-events".to_string(),
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
            "BatchQueryRequest".to_string(),
            RefOr::T(Schema::Object(
                ObjectBuilder::new()
                    .property(
                        "wordIds",
                        RefOr::T(Schema::Array(
                            ArrayBuilder::new().items(string_schema()).build(),
                        )),
                    )
                    .required("wordIds")
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
            "ResourcePackInstallReport".to_string(),
            RefOr::T(Schema::Object(
                ObjectBuilder::new()
                    .property("packId", string_schema())
                    .property("version", string_schema())
                    .property("outcome", string_schema())
                    .property("appVersion", string_schema())
                    .required("packId")
                    .required("version")
                    .required("outcome")
                    .build(),
            )),
        ),
        (
            "AppEventsRequest".to_string(),
            RefOr::T(Schema::Object(
                ObjectBuilder::new()
                    .property(
                        "events",
                        RefOr::T(Schema::Array(
                            ArrayBuilder::new()
                                .items(RefOr::T(Schema::Object(
                                    ObjectBuilder::new()
                                        .property("clientEventId", string_schema())
                                        .property("name", string_schema())
                                        .property(
                                            "category",
                                            RefOr::T(Schema::Object(
                                                ObjectBuilder::new()
                                                    .schema_type(SchemaType::Type(Type::String))
                                                    .enum_values(Some([
                                                        "behavior", "error", "perf",
                                                    ]))
                                                    .build(),
                                            )),
                                        )
                                        .property("clientTsMs", integer_schema())
                                        .property(
                                            "props",
                                            RefOr::T(Schema::Object(
                                                ObjectBuilder::new().build(),
                                            )),
                                        )
                                        .required("clientEventId")
                                        .required("name")
                                        .required("category")
                                        .required("clientTsMs")
                                        .build(),
                                )))
                                .build(),
                        )),
                    )
                    .required("events")
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
/// 端点按路径分组（同路径多方法合并为一个 PathItem）；具体覆盖以本列表为准，
/// 不在注释里维护计数（历次演进后具体数字必然漂移）。
pub fn build() -> utoipa::openapi::OpenApi {
    let paths_list: Vec<(String, utoipa::openapi::PathItem)> = vec![
        // auth
        path_auth_register(),
        path_auth_login(),
        path_auth_refresh(),
        path_auth_logout(),
        path_auth_forgot_password(),
        path_auth_reset_password(),
        // user（GET+PUT 同路径）
        path_user_profile(),
        // words
        path_words(),
        path_words_id(),
        // learning
        path_records(),
        path_learning_session(),
        path_learning_sessions_list(),
        // word-states（GET 单个 + 读侧 batch/due/stats + 3 个独立写操作，无 PUT——契约修正见函数注释）
        path_word_states_id(),
        path_word_states_mark_mastered(),
        path_word_states_reset(),
        path_word_states_batch_update(),
        path_word_states_batch_query(),
        path_word_states_due_list(),
        path_word_states_stats_overview(),
        // favorites
        path_word_favorites(),
        path_word_favorites_status(),
        path_word_favorites_id(),
        // resource-packs（客户端公开端点，此前整块缺失）
        path_resource_packs_list(),
        path_resource_packs_public_key(),
        path_resource_packs_manifest(),
        // telemetry（此前整块缺失，此处仅补 finding 指出的安装上报这一个端点）
        path_telemetry_resource_pack_install(),
        path_telemetry_app_events(),
        // notes（GET+POST 同路径）
        path_word_notes(),
        // wordbooks
        path_wordbooks(),
        // config（GET+PUT 同路径）
        path_study_config(),
        // analytics
        path_analytics_dashboard(),
        // realtime SSE
        path_realtime_sse(),
        // misc
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
