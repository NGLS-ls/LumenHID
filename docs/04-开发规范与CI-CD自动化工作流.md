# 04. 开发规范与 GitHub Actions 自动化编译工作流

**文档编号**：LUMEN-DEV-2026-04  
**版本**：v1.0.0  
**适用对象**：前端工程师、Rust 系统工程师、发布与 QA 团队

---

## 1. 仓库目录结构规范

```text
lumenhid/
├── .github/
│   └── workflows/
│       └── build-portable-exe.yml   # GitHub Actions 自动化质检与免安装 EXE 构建流水线
├── docs/                            # 全生命周期标准化研发文档（01~05）
├── src/                             # React 19 + TypeScript 前端应用
│   ├── components/                  # 按功能域拆分的低耦合视图模块
│   ├── data/                        # 默认设备种子、预设与文档索引
│   ├── services/                    # Tauri IPC / WebHID / Web Audio 桥接层
│   ├── types/                       # 全局领域类型定义
│   └── utils/                       # 纯函数业务逻辑引擎与 node:test 单元测试
└── src-tauri/                       # Tauri v2 Rust 原生硬件核心
    ├── Cargo.toml                   # LTO/Strip 便携构建配置
    ├── tauri.conf.json              # 窗口尺寸、安全 CSP 与打包配置
    └── src/
        ├── main.rs                  # Windows 子系统入口
        ├── lib.rs                   # Tauri IPC 命令与便携数据目录管理
        └── hid_protocol.rs          # 16 字节 HID Feature Report 封包与 XOR 校验
```

---

## 2. 编码与提交规范

1. **逻辑与 UI 严格解耦**：所有 DPI 夹逼、HID 报文编码、磁轴行程校验、宏时间轴标准化、EQ 贝塞尔曲线生成与配置反序列化均必须沉淀至 `src/utils/deviceEngine.ts`，禁止在 JSX 组件内堆砌复杂计算。
2. **类型安全零宽容**：TypeScript 严格通过 `npm run lint`（`tsc --noEmit`），Rust 端通过 `cargo clippy -- -D warnings` 门禁。
3. **语义化提交（Conventional Commits）**：
   - `feat(mouse):` 新增鼠标功能
   - `fix(macro):` 修复宏调度缺陷
   - `docs(sdlc):` 更新研发全流程文档
   - `ci(tauri):` 调整自动化编译工作流

---

## 3. GitHub Actions 免安装 EXE 自动化编译工作流

仓库已内置标准流水线 `.github/workflows/build-portable-exe.yml`，每次推送 `main` 分支、创建 `v*.*.*` 标签或手动触发（`workflow_dispatch`）时自动执行以下两阶段流水线：

### 阶段一：质量门禁（`quality-gate`，Ubuntu Runner）
1. 检出源码并安装 Node.js 22 依赖缓存。
2. 执行 TypeScript 静态类型检查：`npm run lint`。
3. 执行核心逻辑单元测试：`npx tsx --test src/utils/deviceEngine.test.ts`。
4. 执行前端生产构建：`npm run build`，验证零警告产出 `dist/`。

### 阶段二：Windows 免安装便携版编译与打包（`build-windows-portable`，`windows-latest`）
1. 仅在阶段一全部通过后触发，安装 Rust stable 工具链与 `x86_64-pc-windows-msvc` 目标。
2. 启用 Cargo 增量缓存（`Swatinem/rust-cache`），大幅缩短后续构建耗时。
3. 执行 `cargo build --release --manifest-path src-tauri/Cargo.toml`（配合 Tauri v2 `tauri build --no-bundle`），直接输出独立可执行文件 `lumen-hid.exe`。
4. 自动组装便携分发包 `LumenHID-Portable-win-x64.zip`：
   - `LumenHID.exe`（免安装主程序）
   - `portable.flag`（便携模式锚点标识）
   - `README-Portable.txt`（免安装使用与 WebView2 运行时说明）
   - `SHA256SUMS.txt`（完整性校验指纹）
5. 上传 GitHub Actions Artifact；若由 `v*` Release Tag 触发，则自动发布至 GitHub Releases 供最终用户一键下载。

---

## 4. 本地调试与离线打包命令

```bash
# 1. 前端本地开发预览（端口 3000）
npm install && npm run dev

# 2. 运行核心逻辑单元测试
npx tsx --test src/utils/deviceEngine.test.ts

# 3. 本地编译 Tauri 免安装 EXE（需 Windows + Rust + WebView2）
npx tauri build --no-bundle
# 产物位置：src-tauri/target/release/lumen-hid.exe
```
