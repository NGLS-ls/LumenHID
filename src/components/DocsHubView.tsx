import { useState, type ReactNode } from 'react';
import { Check, Copy, Download, FileCode2, FileText } from 'lucide-react';
import { SDLC_DOCS } from '../data/sdlcDocs';
import workflowYml from '../../.github/workflows/build-portable-exe.yml?raw';
import tauriConf from '../../src-tauri/tauri.conf.json?raw';
import cargoToml from '../../src-tauri/Cargo.toml?raw';
import { downloadTextFile } from '../services/deviceBridge';

type ArtifactTab = 'docs' | 'workflow' | 'tauri';

function renderInlineFormatting(text: string): ReactNode[] {
  const parts = text.split(/(`[^`]+`|\*\*[^*]+\*\*)/g);
  return parts.map((part, idx) => {
    if (part.startsWith('`') && part.endsWith('`')) {
      return (
        <code
          key={idx}
          className="rounded bg-[#F0F3F6] px-1.5 py-0.5 font-mono text-[12px] text-[#1D4E4C]"
        >
          {part.slice(1, -1)}
        </code>
      );
    }
    if (part.startsWith('**') && part.endsWith('**')) {
      return (
        <strong key={idx} className="font-semibold text-[#181C20]">
          {part.slice(2, -2)}
        </strong>
      );
    }
    return part;
  });
}

function SimpleMarkdownView({ markdown }: { markdown: string }) {
  const lines = markdown.replace(/\r\n/g, '\n').split('\n');
  const blocks: ReactNode[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Code block
    if (line.trim().startsWith('```')) {
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trim().startsWith('```')) {
        codeLines.push(lines[i]);
        i++;
      }
      i++;
      blocks.push(
        <pre
          key={`code-${i}`}
          className="my-4 overflow-x-auto rounded-xl border border-[#E5E9EE] bg-[#F7F8FA] p-4 font-mono text-xs leading-relaxed text-[#181C20]"
        >
          <code>{codeLines.join('\n')}</code>
        </pre>
      );
      continue;
    }

    // Table block
    if (line.trim().startsWith('|') && i + 1 < lines.length && lines[i + 1].trim().startsWith('|')) {
      const tableLines: string[] = [];
      while (i < lines.length && lines[i].trim().startsWith('|')) {
        tableLines.push(lines[i]);
        i++;
      }
      const parseRow = (row: string) =>
        row
          .trim()
          .replace(/^\|/, '')
          .replace(/\|$/, '')
          .split('|')
          .map((cell) => cell.trim());

      const headers = parseRow(tableLines[0]);
      const rows = tableLines.slice(2).map(parseRow);

      blocks.push(
        <div key={`tbl-${i}`} className="my-4 overflow-x-auto rounded-xl border border-[#E5E9EE]">
          <table className="w-full text-left text-xs">
            <thead className="bg-[#F7F8FA] text-[#586370]">
              <tr>
                {headers.map((h, hIdx) => (
                  <th key={hIdx} className="px-3.5 py-2.5 font-semibold whitespace-nowrap">
                    {renderInlineFormatting(h)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody className="divide-y divide-[#EEF1F4] bg-white">
              {rows.map((r, rIdx) => (
                <tr key={rIdx} className="hover:bg-[#FAFBFD]">
                  {r.map((cell, cIdx) => (
                    <td key={cIdx} className="px-3.5 py-2.5 text-[#343C45] align-top">
                      {renderInlineFormatting(cell)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
      continue;
    }

    // Horizontal rule
    if (line.trim() === '---') {
      blocks.push(<hr key={`hr-${i}`} className="my-6 border-[#EEF1F4]" />);
      i++;
      continue;
    }

    // Headings
    if (line.startsWith('# ')) {
      blocks.push(
        <h2 key={`h1-${i}`} className="mt-2 mb-3 text-xl font-semibold tracking-tight text-[#181C20]">
          {line.slice(2)}
        </h2>
      );
      i++;
      continue;
    }
    if (line.startsWith('## ')) {
      blocks.push(
        <h3 key={`h2-${i}`} className="mt-6 mb-2.5 text-base font-semibold text-[#181C20]">
          {line.slice(3)}
        </h3>
      );
      i++;
      continue;
    }
    if (line.startsWith('### ')) {
      blocks.push(
        <h4 key={`h3-${i}`} className="mt-4 mb-2 text-sm font-semibold text-[#1D4E4C]">
          {line.slice(4)}
        </h4>
      );
      i++;
      continue;
    }

    // Unordered or checklist items
    if (/^\s*-\s+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\s*-\s+/.test(lines[i])) {
        items.push(lines[i].replace(/^\s*-\s+/, ''));
        i++;
      }
      blocks.push(
        <ul key={`ul-${i}`} className="my-2.5 space-y-1.5 pl-4 text-sm leading-relaxed text-[#343C45] list-disc">
          {items.map((item, idx) => (
            <li key={idx}>{renderInlineFormatting(item)}</li>
          ))}
        </ul>
      );
      continue;
    }

    // Ordered list
    if (/^\s*\d+\.\s+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\s*\d+\.\s+/.test(lines[i])) {
        items.push(lines[i].replace(/^\s*\d+\.\s+/, ''));
        i++;
      }
      blocks.push(
        <ol key={`ol-${i}`} className="my-2.5 space-y-1.5 pl-5 text-sm leading-relaxed text-[#343C45] list-decimal">
          {items.map((item, idx) => (
            <li key={idx}>{renderInlineFormatting(item)}</li>
          ))}
        </ol>
      );
      continue;
    }

    // Blank line
    if (!line.trim()) {
      i++;
      continue;
    }

    // Normal paragraph
    blocks.push(
      <p key={`p-${i}`} className="my-2 text-sm leading-relaxed text-[#343C45]">
        {renderInlineFormatting(line)}
      </p>
    );
    i++;
  }

  return <article className="max-w-none">{blocks}</article>;
}

export function DocsHubView() {
  const [artifactTab, setArtifactTab] = useState<ArtifactTab>('docs');
  const [activeDocId, setActiveDocId] = useState<string>(SDLC_DOCS[0].id);
  const [copied, setCopied] = useState(false);

  const currentDoc = SDLC_DOCS.find((d) => d.id === activeDocId) ?? SDLC_DOCS[0];

  const handleCopy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      // Clipboard may be restricted in sandbox
    }
  };

  const handleDownloadAllDocs = () => {
    const bundle = SDLC_DOCS.map((d) => d.content).join('\n\n---\n\n');
    downloadTextFile('LumenHID-全流程研发与技术文档合集.md', bundle, 'text/markdown;charset=utf-8');
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight text-[#181C20]">
            工程交付与全流程文档中心
          </h1>
          <p className="mt-1 text-xs text-[#586370]">
            覆盖功能调研、需求定稿、Tauri 架构、GitHub Actions 免安装编译与回归测试的标准化研发产物。
          </p>
        </div>

        <button
          type="button"
          onClick={handleDownloadAllDocs}
          className="inline-flex items-center gap-1.5 self-start rounded-lg bg-[#2B6B68] px-4 py-2 text-xs font-medium text-white hover:bg-[#215452] transition-colors whitespace-nowrap"
        >
          <Download className="h-3.5 w-3.5" />
          <span>导出全套文档 (.md)</span>
        </button>
      </div>

      {/* Mode Switcher */}
      <div
        role="tablist"
        aria-label="切换工程交付物视图"
        className="inline-flex max-w-full overflow-x-auto items-center gap-1 rounded-xl bg-[#EEF1F4] p-1"
      >
        {[
          { id: 'docs' as const, label: '研发全流程文档 (01–05)' },
          { id: 'workflow' as const, label: 'GitHub Actions 编译流水线' },
          { id: 'tauri' as const, label: 'Tauri v2 免安装工程配置' },
        ].map((tab) => {
          const active = artifactTab === tab.id;
          return (
            <button
              key={tab.id}
              type="button"
              role="tab"
              aria-selected={active}
              onClick={() => setArtifactTab(tab.id)}
              className={`rounded-lg px-3.5 py-1.5 text-xs font-medium transition-all duration-150 whitespace-nowrap ${
                active
                  ? 'bg-white text-[#181C20] shadow-xs'
                  : 'text-[#586370] hover:text-[#181C20]'
              }`}
            >
              {tab.label}
            </button>
          );
        })}
      </div>

      {artifactTab === 'docs' && (
        <div className="grid grid-cols-1 gap-6 xl:grid-cols-12">
          {/* Left SDLC Stage List */}
          <nav aria-label="研发阶段文档目录" className="xl:col-span-4 space-y-2">
            {SDLC_DOCS.map((doc) => {
              const active = doc.id === currentDoc.id;
              return (
                <button
                  key={doc.id}
                  type="button"
                  onClick={() => setActiveDocId(doc.id)}
                  className={`w-full rounded-2xl border p-4 text-left transition-colors ${
                    active
                      ? 'border-[#2B6B68] bg-white'
                      : 'border-[#E5E9EE] bg-white/75 hover:border-[#C9D3DC] hover:bg-white'
                  }`}
                >
                  <div className="flex items-center justify-between text-xs">
                    <span className="font-mono font-semibold tabular-nums text-[#2B6B68]">
                      {doc.stageNumber}. {doc.stageLabel}
                    </span>
                    <span className="font-mono text-[11px] text-[#84909D]">{doc.docCode}</span>
                  </div>
                  <div className="mt-1.5 text-sm font-semibold text-[#181C20]">{doc.title}</div>
                  <p className="mt-1 text-xs leading-relaxed text-[#586370]">{doc.summary}</p>
                </button>
              );
            })}
          </nav>

          {/* Right Document Reader */}
          <section className="rounded-2xl border border-[#E5E9EE] bg-white p-6 sm:p-8 xl:col-span-8">
            <div className="flex flex-wrap items-center justify-between gap-3 border-b border-[#EEF1F4] pb-4 mb-5">
              <div className="flex items-center gap-2 text-xs text-[#586370]">
                <FileText className="h-4 w-4 text-[#2B6B68]" />
                <span className="font-mono">{currentDoc.filename}</span>
              </div>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => handleCopy(currentDoc.content)}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-[#E5E9EE] px-3 py-1.5 text-xs font-medium text-[#586370] hover:bg-[#F0F3F6] hover:text-[#181C20] whitespace-nowrap"
                >
                  {copied ? <Check className="h-3.5 w-3.5 text-[#2B6B68]" /> : <Copy className="h-3.5 w-3.5" />}
                  <span>{copied ? '已复制' : '复制原文'}</span>
                </button>
                <button
                  type="button"
                  onClick={() =>
                    downloadTextFile(
                      currentDoc.filename,
                      currentDoc.content,
                      'text/markdown;charset=utf-8'
                    )
                  }
                  className="inline-flex items-center gap-1.5 rounded-lg border border-[#E5E9EE] px-3 py-1.5 text-xs font-medium text-[#181C20] hover:bg-[#F0F3F6] whitespace-nowrap"
                >
                  <Download className="h-3.5 w-3.5" />
                  <span>下载此文档</span>
                </button>
              </div>
            </div>

            <SimpleMarkdownView markdown={currentDoc.content} />
          </section>
        </div>
      )}

      {artifactTab === 'workflow' && (
        <section className="rounded-2xl border border-[#E5E9EE] bg-white p-6">
          <div className="flex flex-wrap items-center justify-between gap-3 border-b border-[#EEF1F4] pb-4">
            <div className="flex items-center gap-2">
              <FileCode2 className="h-4 w-4 text-[#2B6B68]" />
              <div>
                <h2 className="text-sm font-semibold text-[#181C20]">
                  .github/workflows/build-portable-exe.yml
                </h2>
                <p className="text-xs text-[#586370]">
                  推送至 main 或打 v* 标签后自动执行 TypeScript 检查、单测与 Windows x64 免安装 EXE 编译打包。
                </p>
              </div>
            </div>
            <button
              type="button"
              onClick={() =>
                downloadTextFile('build-portable-exe.yml', workflowYml, 'text/yaml;charset=utf-8')
              }
              className="inline-flex items-center gap-1.5 rounded-lg border border-[#E5E9EE] px-3 py-1.5 text-xs font-medium text-[#181C20] hover:bg-[#F0F3F6] whitespace-nowrap"
            >
              <Download className="h-3.5 w-3.5" />
              <span>下载工作流 YAML</span>
            </button>
          </div>
          <pre className="mt-4 max-h-[560px] overflow-auto rounded-xl border border-[#E5E9EE] bg-[#F7F8FA] p-4 font-mono text-xs leading-relaxed text-[#181C20]">
            <code>{workflowYml}</code>
          </pre>
        </section>
      )}

      {artifactTab === 'tauri' && (
        <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
          <section className="rounded-2xl border border-[#E5E9EE] bg-white p-6">
            <div className="flex items-center justify-between border-b border-[#EEF1F4] pb-3">
              <h2 className="font-mono text-xs font-semibold text-[#181C20]">
                src-tauri/tauri.conf.json
              </h2>
              <button
                type="button"
                onClick={() =>
                  downloadTextFile('tauri.conf.json', tauriConf, 'application/json;charset=utf-8')
                }
                className="text-xs font-medium text-[#2B6B68] hover:underline"
              >
                下载配置
              </button>
            </div>
            <pre className="mt-4 overflow-auto rounded-xl bg-[#F7F8FA] p-4 font-mono text-xs leading-relaxed text-[#181C20]">
              <code>{tauriConf}</code>
            </pre>
          </section>

          <section className="rounded-2xl border border-[#E5E9EE] bg-white p-6">
            <div className="flex items-center justify-between border-b border-[#EEF1F4] pb-3">
              <h2 className="font-mono text-xs font-semibold text-[#181C20]">
                src-tauri/Cargo.toml
              </h2>
              <button
                type="button"
                onClick={() =>
                  downloadTextFile('Cargo.toml', cargoToml, 'text/plain;charset=utf-8')
                }
                className="text-xs font-medium text-[#2B6B68] hover:underline"
              >
                下载配置
              </button>
            </div>
            <pre className="mt-4 overflow-auto rounded-xl bg-[#F7F8FA] p-4 font-mono text-xs leading-relaxed text-[#181C20]">
              <code>{cargoToml}</code>
            </pre>
          </section>
        </div>
      )}
    </div>
  );
}
