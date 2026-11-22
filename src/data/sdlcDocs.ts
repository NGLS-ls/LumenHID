import doc01Research from '../../docs/01-市场与功能调研报告.md?raw';
import doc02Prd from '../../docs/02-产品需求规格说明书-PRD.md?raw';
import doc03Arch from '../../docs/03-系统架构与技术方案.md?raw';
import doc04DevCicd from '../../docs/04-开发规范与CI-CD自动化工作流.md?raw';
import doc05Qa from '../../docs/05-测试计划与回归验证报告.md?raw';

export interface SdlcDocEntry {
  id: string;
  stageNumber: string;
  stageLabel: string;
  title: string;
  filename: string;
  docCode: string;
  summary: string;
  content: string;
}

export const SDLC_DOCS: SdlcDocEntry[] = [
  {
    id: 'research',
    stageNumber: '01',
    stageLabel: '功能调研',
    title: '市场竞品与外设驱动功能调研报告',
    filename: '01-市场与功能调研报告.md',
    docCode: 'LUMEN-RES-2026-01',
    summary: '对标主流桌面驱动套件，梳理 2.4G、蓝牙、USB 全协议痛点与高频核心功能矩阵。',
    content: doc01Research,
  },
  {
    id: 'prd',
    stageNumber: '02',
    stageLabel: '需求定稿',
    title: '产品需求规格说明书（PRD）',
    filename: '02-产品需求规格说明书-PRD.md',
    docCode: 'LUMEN-PRD-2026-02',
    summary: '界定极简淡雅视觉规范、五大核心功能模块需求（FR-01~05）及免安装性能指标。',
    content: doc02Prd,
  },
  {
    id: 'architecture',
    stageNumber: '03',
    stageLabel: '技术方案',
    title: '系统架构与技术方案设计书',
    filename: '03-系统架构与技术方案.md',
    docCode: 'LUMEN-ARCH-2026-03',
    summary: '详述 Tauri v2 + Rust 硬件层、Portable 零注册表存储机制与 16 字节 HID 通信帧格式。',
    content: doc03Arch,
  },
  {
    id: 'cicd',
    stageNumber: '04',
    stageLabel: '开发与 CI/CD',
    title: '开发规范与 GitHub Actions 自动化编译工作流',
    filename: '04-开发规范与CI-CD自动化工作流.md',
    docCode: 'LUMEN-DEV-2026-04',
    summary: '规范仓库目录、代码解耦门禁及 Windows 免安装 EXE 自动化编译打包与发版流水线。',
    content: doc04DevCicd,
  },
  {
    id: 'qa',
    stageNumber: '05',
    stageLabel: '测试与回归',
    title: '测试计划与回归验证报告',
    filename: '05-测试计划与回归验证报告.md',
    docCode: 'LUMEN-QA-2026-05',
    summary: '覆盖 node:test 单元测试矩阵、E2E 关键用户旅程用例及 Windows 实机发版签核清单。',
    content: doc05Qa,
  },
];
