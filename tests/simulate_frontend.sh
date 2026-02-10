#!/bin/bash
# =============================================================
# 后端模拟测试脚本 - 模拟前端真实用户操作
# =============================================================

BASE="http://127.0.0.1:3001/api"
PASS=0
FAIL=0
TOTAL=0

green() { echo -e "\033[32m✅ PASS: $1\033[0m"; PASS=$((PASS+1)); TOTAL=$((TOTAL+1)); }
red()   { echo -e "\033[31m❌ FAIL: $1 — $2\033[0m"; FAIL=$((FAIL+1)); TOTAL=$((TOTAL+1)); }
header(){ echo -e "\n\033[1;34m========== $1 ==========\033[0m"; }

check() {
  local name="$1" expected_code="$2" actual_code="$3" body="$4"
  if [ "$actual_code" == "$expected_code" ]; then
    green "$name (HTTP $actual_code)"
  else
    red "$name" "期望 HTTP $expected_code, 实际 $actual_code | body: $(echo "$body" | head -c 200)"
  fi
}

# =============================================================
header "0. 健康检查"
# =============================================================
RESP=$(curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:3001/health/live)
check "GET /health/live" "200" "$RESP"

RESP=$(curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:3001/health/ready)
check "GET /health/ready" "200" "$RESP"

# =============================================================
header "1. 用户注册"
# =============================================================

# 1a. 正常注册
BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/auth/register" \
  -H "Content-Type: application/json" \
  -d '{"email":"test@example.com","username":"TestUser","password":"SecurePass123"}')
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "POST /auth/register 正常注册" "201" "$CODE" "$RESP"
TOKEN=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['token'])" 2>/dev/null)
echo "  → 获取到 Token: ${TOKEN:0:30}..."

# 1b. 重复邮箱注册 (应失败 409)
BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/auth/register" \
  -H "Content-Type: application/json" \
  -d '{"email":"test@example.com","username":"Another","password":"SecurePass123"}')
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "POST /auth/register 重复邮箱 → 409" "409" "$CODE" "$RESP"

# 1c. 弱密码 (应失败 400)
BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/auth/register" \
  -H "Content-Type: application/json" \
  -d '{"email":"weak@example.com","username":"Weak","password":"123"}')
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "POST /auth/register 弱密码 → 400" "400" "$CODE" "$RESP"

# 1d. 无效邮箱 (应失败 400)
BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/auth/register" \
  -H "Content-Type: application/json" \
  -d '{"email":"not-an-email","username":"Bad","password":"SecurePass123"}')
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "POST /auth/register 无效邮箱 → 400" "400" "$CODE" "$RESP"

# =============================================================
header "2. 用户登录"
# =============================================================

# 2a. 正常登录
BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"email":"test@example.com","password":"SecurePass123"}')
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "POST /auth/login 正常登录" "200" "$CODE" "$RESP"
TOKEN=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['token'])" 2>/dev/null)
REFRESH=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['refreshToken'])" 2>/dev/null)
echo "  → Token: ${TOKEN:0:30}..."

# 2b. 错误密码 (应失败 401)
BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"email":"test@example.com","password":"WrongPassword"}')
CODE=$(echo "$BODY" | tail -1)
check "POST /auth/login 错误密码 → 401" "401" "$CODE"

# 2c. 不存在的用户 (应失败 401)
BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"email":"noone@example.com","password":"SomePass123"}')
CODE=$(echo "$BODY" | tail -1)
check "POST /auth/login 用户不存在 → 401" "401" "$CODE"

# =============================================================
header "3. 用户信息 (需认证)"
# =============================================================

# 3a. 获取个人资料
BODY=$(curl -s -w "\n%{http_code}" -X GET "$BASE/users/me" \
  -H "Authorization: Bearer $TOKEN")
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "GET /users/me 获取资料" "200" "$CODE" "$RESP"
echo "  → 用户信息: $(echo "$RESP" | python3 -c "import sys,json; d=json.load(sys.stdin)['data']; print(f\"id={d['id'][:8]}... username={d['username']} email={d['email']}\")" 2>/dev/null)"

# 3b. 无 Token 访问 (应失败 401)
BODY=$(curl -s -w "\n%{http_code}" -X GET "$BASE/users/me")
CODE=$(echo "$BODY" | tail -1)
check "GET /users/me 无Token → 401" "401" "$CODE"

# 3c. 修改用户名
BODY=$(curl -s -w "\n%{http_code}" -X PUT "$BASE/users/me" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"username":"UpdatedName"}')
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "PUT /users/me 修改用户名" "200" "$CODE" "$RESP"

# 3d. 获取学习统计
BODY=$(curl -s -w "\n%{http_code}" -X GET "$BASE/users/me/stats" \
  -H "Authorization: Bearer $TOKEN")
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "GET /users/me/stats 学习统计" "200" "$CODE" "$RESP"
echo "  → 统计: $(echo "$RESP" | python3 -c "import sys,json; print(json.dumps(json.load(sys.stdin)['data'], indent=2))" 2>/dev/null)"

# =============================================================
header "4. 单词管理 (无需认证)"
# =============================================================

# 4a. 创建单词
BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/words" \
  -H "Content-Type: application/json" \
  -d '{
    "text": "apple",
    "meaning": "苹果，一种常见水果",
    "pronunciation": "ˈæp.əl",
    "partOfSpeech": "noun",
    "difficulty": 0.3,
    "examples": ["I eat an apple every day.", "The apple fell from the tree."],
    "tags": ["fruit", "food", "beginner"]
  }')
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "POST /words 创建单词 apple" "201" "$CODE" "$RESP"
WORD1_ID=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['id'])" 2>/dev/null)
echo "  → Word ID: $WORD1_ID"

# 4b. 创建第二个单词
BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/words" \
  -H "Content-Type: application/json" \
  -d '{
    "text": "ephemeral",
    "meaning": "短暂的，转瞬即逝的",
    "pronunciation": "ɪˈfem.ər.əl",
    "partOfSpeech": "adjective",
    "difficulty": 0.8,
    "examples": ["Fame is ephemeral.", "The beauty of cherry blossoms is ephemeral."],
    "tags": ["advanced", "abstract"]
  }')
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "POST /words 创建单词 ephemeral" "201" "$CODE" "$RESP"
WORD2_ID=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['id'])" 2>/dev/null)

# 4c. 批量创建单词
BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/words/batch" \
  -H "Content-Type: application/json" \
  -d '{
    "words": [
      {"text":"banana","meaning":"香蕉","difficulty":0.2,"tags":["fruit","food"]},
      {"text":"serendipity","meaning":"意外发现珍奇事物的运气","difficulty":0.9,"partOfSpeech":"noun","tags":["advanced"]},
      {"text":"ubiquitous","meaning":"无处不在的","pronunciation":"juːˈbɪk.wɪ.təs","difficulty":0.7,"partOfSpeech":"adjective","examples":["Smartphones are ubiquitous."],"tags":["advanced"]},
      {"text":"","meaning":""},
      {"text":"resilient","meaning":"有弹性的，能恢复的","difficulty":0.6,"tags":["intermediate"]}
    ]
  }')
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "POST /words/batch 批量创建" "201" "$CODE" "$RESP"
BATCH_COUNT=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['count'])" 2>/dev/null)
echo "  → 批量创建 $BATCH_COUNT 个单词 (包含1个空数据应被跳过)"

# 4d. 获取单词列表
BODY=$(curl -s -w "\n%{http_code}" -X GET "$BASE/words?limit=10&offset=0")
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "GET /words 获取单词列表" "200" "$CODE" "$RESP"
WORD_COUNT=$(echo "$RESP" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['items']))" 2>/dev/null)
echo "  → 当前共有 $WORD_COUNT 个单词"

# 4e. 获取单个单词
BODY=$(curl -s -w "\n%{http_code}" -X GET "$BASE/words/$WORD1_ID")
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "GET /words/:id 获取单词详情" "200" "$CODE" "$RESP"

# 4f. 更新单词
BODY=$(curl -s -w "\n%{http_code}" -X PUT "$BASE/words/$WORD1_ID" \
  -H "Content-Type: application/json" \
  -d '{
    "text": "apple",
    "meaning": "苹果 — 蔷薇科落叶乔木果实",
    "pronunciation": "ˈæp.əl",
    "partOfSpeech": "noun",
    "difficulty": 0.25,
    "examples": ["I eat an apple every day.", "The apple fell from the tree.", "An apple a day keeps the doctor away."],
    "tags": ["fruit", "food", "beginner", "proverb"]
  }')
CODE=$(echo "$BODY" | tail -1)
check "PUT /words/:id 更新单词" "200" "$CODE"

# 4g. 无效 limit (应失败 400)
BODY=$(curl -s -w "\n%{http_code}" -X GET "$BASE/words?limit=0")
CODE=$(echo "$BODY" | tail -1)
check "GET /words?limit=0 无效limit → 400" "400" "$CODE"

# 4h. 不存在的单词 (应失败 404)
BODY=$(curl -s -w "\n%{http_code}" -X GET "$BASE/words/nonexistent-id-12345")
CODE=$(echo "$BODY" | tail -1)
check "GET /words/:id 不存在 → 404" "404" "$CODE"

# =============================================================
header "5. 学习记录 (模拟前端学习会话)"
# =============================================================

SESSION_ID="sim-session-$(date +%s)"

# 5a. 提交正确答案 - 快速回答
BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/records" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"wordId\": \"$WORD1_ID\",
    \"isCorrect\": true,
    \"responseTimeMs\": 1200,
    \"sessionId\": \"$SESSION_ID\",
    \"dwellTimeMs\": 3000,
    \"pauseCount\": 0,
    \"switchCount\": 0,
    \"retryCount\": 0,
    \"focusLossDurationMs\": 0,
    \"interactionDensity\": 0.9,
    \"pausedTimeMs\": 0,
    \"hintUsed\": false
  }")
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "POST /records 正确答案(快速)" "201" "$CODE" "$RESP"
echo "  → AMAS 结果: $(echo "$RESP" | python3 -c "import sys,json; r=json.load(sys.stdin)['data']; print('record_id=' + r['record']['id'][:8] + '...')" 2>/dev/null)"

# 5b. 提交错误答案 - 慢速回答
BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/records" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"wordId\": \"$WORD2_ID\",
    \"isCorrect\": false,
    \"responseTimeMs\": 8500,
    \"sessionId\": \"$SESSION_ID\",
    \"dwellTimeMs\": 12000,
    \"pauseCount\": 3,
    \"switchCount\": 1,
    \"retryCount\": 1,
    \"focusLossDurationMs\": 2000,
    \"interactionDensity\": 0.4,
    \"pausedTimeMs\": 3000,
    \"hintUsed\": true
  }")
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "POST /records 错误答案(慢速)" "201" "$CODE" "$RESP"

# 5c. 提交多轮学习记录 (模拟完整学习会话)
for i in 1 2 3 4 5; do
  CORRECT=$( [ $((RANDOM % 3)) -ne 0 ] && echo "true" || echo "false" )
  RT=$((800 + RANDOM % 5000))
  BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/records" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{
      \"wordId\": \"$WORD1_ID\",
      \"isCorrect\": $CORRECT,
      \"responseTimeMs\": $RT,
      \"sessionId\": \"$SESSION_ID\",
      \"dwellTimeMs\": $((RT + 1000)),
      \"pauseCount\": $((RANDOM % 3)),
      \"switchCount\": 0,
      \"retryCount\": 0,
      \"hintUsed\": false
    }")
  CODE=$(echo "$BODY" | tail -1)
  check "POST /records 学习轮次 #$i (correct=$CORRECT, rt=${RT}ms)" "201" "$CODE"
done

# 5d. 无 Token 提交记录 (应失败 401)
BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/records" \
  -H "Content-Type: application/json" \
  -d '{"wordId":"any","isCorrect":true,"responseTimeMs":1000}')
CODE=$(echo "$BODY" | tail -1)
check "POST /records 无Token → 401" "401" "$CODE"

# 5e. 获取学习记录
BODY=$(curl -s -w "\n%{http_code}" -X GET "$BASE/records?limit=20" \
  -H "Authorization: Bearer $TOKEN")
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "GET /records 获取学习记录" "200" "$CODE" "$RESP"
REC_COUNT=$(echo "$RESP" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']))" 2>/dev/null)
echo "  → 共有 $REC_COUNT 条学习记录"

# =============================================================
header "6. Token 刷新"
# =============================================================

BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/auth/refresh" \
  -H "Authorization: Bearer $REFRESH")
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "POST /auth/refresh 刷新Token" "200" "$CODE" "$RESP"
NEW_TOKEN=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['accessToken'])" 2>/dev/null)
NEW_REFRESH=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['refreshToken'])" 2>/dev/null)
if [ -n "$NEW_TOKEN" ] && [ "$NEW_TOKEN" != "None" ]; then
  echo "  → 新 Token: ${NEW_TOKEN:0:30}..."
  TOKEN="$NEW_TOKEN"
  REFRESH="$NEW_REFRESH"
fi

# =============================================================
header "7. 修改密码"
# =============================================================

BODY=$(curl -s -w "\n%{http_code}" -X PUT "$BASE/users/me/password" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"current_password":"SecurePass123","new_password":"NewSecure456"}')
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "PUT /users/me/password 修改密码" "200" "$CODE" "$RESP"

# 7b. 用新密码重新登录
BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"email":"test@example.com","password":"NewSecure456"}')
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "POST /auth/login 新密码登录" "200" "$CODE" "$RESP"
TOKEN=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['token'])" 2>/dev/null)

# 7c. 旧密码应失败
BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"email":"test@example.com","password":"SecurePass123"}')
CODE=$(echo "$BODY" | tail -1)
check "POST /auth/login 旧密码 → 401" "401" "$CODE"

# =============================================================
header "8. 忘记密码流程"
# =============================================================

BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/auth/forgot-password" \
  -H "Content-Type: application/json" \
  -d '{"email":"test@example.com"}')
CODE=$(echo "$BODY" | tail -1)
check "POST /auth/forgot-password" "200" "$CODE"

# =============================================================
header "9. 再次查看学习统计 (有数据后)"
# =============================================================

BODY=$(curl -s -w "\n%{http_code}" -X GET "$BASE/users/me/stats" \
  -H "Authorization: Bearer $TOKEN")
CODE=$(echo "$BODY" | tail -1)
RESP=$(echo "$BODY" | sed '$d')
check "GET /users/me/stats 更新后统计" "200" "$CODE" "$RESP"
echo "  → 统计: $(echo "$RESP" | python3 -c "import sys,json; print(json.dumps(json.load(sys.stdin)['data'], indent=2))" 2>/dev/null)"

# =============================================================
header "10. 注销登录"
# =============================================================

BODY=$(curl -s -w "\n%{http_code}" -X POST "$BASE/auth/logout" \
  -H "Authorization: Bearer $TOKEN")
CODE=$(echo "$BODY" | tail -1)
check "POST /auth/logout 注销" "200" "$CODE"

# 注销后访问应失败
BODY=$(curl -s -w "\n%{http_code}" -X GET "$BASE/users/me" \
  -H "Authorization: Bearer $TOKEN")
CODE=$(echo "$BODY" | tail -1)
check "GET /users/me 注销后 → 401" "401" "$CODE"

# =============================================================
header "11. 404 回退"
# =============================================================

BODY=$(curl -s -w "\n%{http_code}" -X GET "http://127.0.0.1:3001/nonexistent")
CODE=$(echo "$BODY" | tail -1)
check "GET /nonexistent → 404" "404" "$CODE"

# =============================================================
echo ""
echo "=========================================="
echo -e "\033[1m📊 测试结果汇总\033[0m"
echo "=========================================="
echo -e "总计: $TOTAL 项测试"
echo -e "\033[32m通过: $PASS\033[0m"
echo -e "\033[31m失败: $FAIL\033[0m"
if [ "$FAIL" -eq 0 ]; then
  echo -e "\n\033[1;32m🎉 全部通过！后端可以正常处理前端请求。\033[0m"
else
  echo -e "\n\033[1;31m⚠️  有 $FAIL 项测试失败，请检查上方详细信息。\033[0m"
fi
echo "=========================================="
