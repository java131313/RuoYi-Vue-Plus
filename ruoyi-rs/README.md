# ruoyi-rs — RuoYi-Vue-Plus 后端 Rust 重写版

将 RuoYi-Vue-Plus（Java/Spring Boot + MySQL + Redis）的**核心 system 模块**用 Rust 重写：
**前端 plus-ui 完全不变**，SQLite 内嵌替代 MySQL + Redis，单容器一体化部署。

- Web 框架：axum + tokio（依赖极少，全部纯 Rust，可 musl 静态编译）
- 数据库：SQLite（`rusqlite` bundled，首次启动自动建表灌入种子数据）
- 认证：JWT(HS256) + BCrypt，兼容原 Sa-Token 的 `Authorization: Bearer` 契约
- 登录安全：与原版一致的 RSA+AES 请求体解密（plus-ui `isEncrypt`）、数学验证码（手写 PNG，无图形库）、密码错误锁定（5 次/10 分钟）
- 镜像：distroless 静态运行时，前端在构建期从 [plus-ui](https://github.com/JavaLionLi/plus-ui) 官方仓库 `5.X` 分支构建

## 已实现功能（与 plus-ui 5.X 接口契约一致）

| 模块 | 接口 |
|---|---|
| 认证 | `/auth/login` `/auth/logout` `/auth/code` `/auth/tenant/list` `/auth/register` |
| 用户 | 增删改查、分页、重置密码、改状态、分配角色(`authRole`)、部门树、数据权限过滤、个人中心/改密/头像上传 |
| 角色 | 增删改查、数据范围(`dataScope`)、改状态、分配用户（allocated/unallocated/cancel/selectAll） |
| 菜单 | 增删改查、treeselect、roleMenuTreeselect、**getRouters 动态路由**（忠实移植 Java buildMenus） |
| 部门 | 树、排除子树、ancestors 级联维护 |
| 岗位/字典/参数/公告 | 全套 CRUD + optionselect + `dict/data/type/{t}`、`config/configKey/{k}` 公共查询 |
| 监控 | 在线用户（内存会话）、登录日志、操作日志（含清空/解锁） |

未实现（菜单种子已裁剪，前端自然不显示）：代码生成器、定时任务、工作流、OSS 对象存储、
多租户切换（固定 `000000`）、SSE/WebSocket 推送、Excel 导入导出。

## 快速开始

### Docker（推荐）

```bash
# 本地构建（vite 构建需要约 4GB 内存，请确保 Docker 分配 ≥ 6GB；GitHub Actions 无此问题）
docker build -t ruoyi-rs ./ruoyi-rs
docker run -d -p 8080:8080 -v ruoyi-data:/ruoyi/data --name ruoyi-rs ruoyi-rs

# 或直接使用 GHCR 镜像（GitHub Actions 自动构建）
docker run -d -p 8080:8080 -v ruoyi-data:/ruoyi/data \
  ghcr.io/java131313/ruoyi-vue-plus-rs:latest
```

打开 http://localhost:8080 ，账号 `admin / admin123`（另有 `test / 666666`、`test1 / 666666` 用于数据权限演示）。

### docker compose

```bash
docker compose -f ruoyi-rs/docker-compose.yml up -d
```

### 本地开发

```bash
cd ruoyi-rs
cargo run                 # 监听 0.0.0.0:8080，自动初始化 data/ruoyi.db
cargo test                # 加密链路 / BCrypt / PNG 单元测试
```

前端联调（plus-ui 源码，Vite 代理已指向 8080）：

```bash
git clone -b 5.X https://github.com/JavaLionLi/plus-ui.git
cd plus-ui && pnpm install && pnpm dev
```

## 配置（环境变量）

| 变量 | 默认值 | 说明 |
|---|---|---|
| `PORT` | `8080` | 监听端口 |
| `DB_PATH` | `data/ruoyi.db`（镜像内 `/ruoyi/data/ruoyi.db`） | SQLite 文件 |
| `WEB_DIR` | `web/dist` | 前端产物目录 |
| `UPLOAD_DIR` | `web/uploads` | 头像等上传文件目录（`/uploads/` 静态服务） |
| `JWT_SECRET` | 与 Java 版 yml 相同的演示值 | **生产必须覆盖** |
| `CAPTCHA_ENABLED` | `true` | 数学验证码开关 |

## CI

[.github/workflows/docker.yml](../.github/workflows/docker.yml)：
push 到 `5.X`（且 `ruoyi-rs/` 有改动）或打 `v*` tag 时，构建 linux/amd64 + linux/arm64
镜像并推送 `ghcr.io/<owner>/ruoyi-vue-plus-rs`（`latest`/分支名/版本号/sha 多标签）；
PR 仅构建不推送。首次 arm64 构建走 QEMU 较慢，后续有缓存加速。

## 目录结构

```
ruoyi-rs/
├── Dockerfile            # 多阶段：node 构建前端 → rust musl 编译 → distroless 运行
├── docker-compose.yml
├── sql/
│   ├── schema.sql        # 由 tools/mysql2sqlite.py 从 script/sql/ry_vue_5.X.sql 转换
│   └── seed.sql          # 种子数据（菜单按已实现功能裁剪）
└── src/
    ├── auth.rs           # 登录/验证码/注册/租户（含 RSA+AES 解密）
    ├── crypto.rs         # RSA-1024(PKCS1v15) + AES-256-ECB 解密，与 plus-ui 完全对应
    ├── captcha.rs        # 手写 PNG 编码器 + 5x7 点阵字体数学验证码（零图像依赖）
    ├── perm.rs           # JWT 登录态提取 / 权限加载 / 数据权限
    ├── system/           # user/role/menu/dept/post/dict/config/notice/profile
    ├── monitor.rs        # 在线用户 / 登录日志 / 操作日志
    └── web.rs            # 静态托管 + SPA 回退 + /uploads
```

## 与 Java 版的差异说明

- 会话/验证码/锁定计数在内存中（单实例），服务重启后登录态失效；JWT 本身无状态
- 雪花 ID 由内置生成器产生；JSON 中 Long 按 RuoYi 规则输出（≤2^53 为数字，更大为字符串）
- 文件上传存储本地磁盘（记录于 `sys_oss` 精简表），不走 MinIO
- 数据库迁移：删除操作对无 `del_flag` 的表（post/dict/config/notice）为物理删除
