/**
 * 源码 ↔ 渲染 的对应关系。
 *
 * 并排对照需要"滚动同步"，但我们没有逐行的渲染映射（分块渲染、懒加载）。
 * 退一步用**标题**做锚点：源码里的 `## 小节` 对应渲染里的 `<h2 id="...">`，
 * 这是两边都稳定存在的唯一对应物。
 */

export interface SourceHeading {
  /** 行号，0 起 */
  line: number;
  level: number;
  text: string;
}

export interface HeadingPair {
  line: number;
  id: string;
  chunk: number;
}

/** 从原始 Markdown 抽标题（跳过围栏代码块，否则 ``` 里的 # 注释会被当成标题） */
export function parseHeadings(text: string): SourceHeading[] {
  const out: SourceHeading[] = [];
  const lines = text.split('\n');
  let fence: string | null = null;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const f = /^\s*(`{3,}|~{3,})/.exec(line);
    if (f) {
      const marker = f[1][0];
      if (fence === null) fence = marker;
      else if (fence === marker) fence = null;
      continue;
    }
    if (fence !== null) continue;

    const h = /^(#{1,6})\s+(\S.*?)\s*#*\s*$/.exec(line);
    if (h) out.push({ line: i, level: h[1].length, text: h[2].trim() });
  }
  return out;
}

/** 归一化：渲染侧是纯文本，源码侧带 **加粗** / `代码` / $公式$ 等标记，去掉后再比 */
function norm(s: string): string {
  return s
    .replace(/[*_`$~\\]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}

/**
 * 按顺序把源码标题与渲染侧 TOC 配对。
 *
 * 先按归一化文本精确匹配；匹配不上时（标题里含 `$\sqrt{\Delta t}$` 这类公式，
 * 渲染侧是 KaTeX 文本、与源码 LaTeX 不同）退回「同层级 + 前 8 个字相同」的位置匹配。
 * 顺序游标只进不退，宁可漏配也不错配——错配会让同步跳到完全不相干的位置。
 */
export function pairHeadings(
  src: SourceHeading[],
  toc: { id: string; text: string; chunk: number; level: number }[],
): HeadingPair[] {
  const out: HeadingPair[] = [];
  let cursor = 0;

  for (const h of src) {
    const want = norm(h.text);
    let matched = -1;

    for (let k = cursor; k < toc.length; k++) {
      if (norm(toc[k].text) === want) {
        matched = k;
        break;
      }
    }

    if (matched < 0 && cursor < toc.length && toc[cursor].level === h.level) {
      const a = want.slice(0, 8);
      const b = norm(toc[cursor].text).slice(0, 8);
      if (a.length >= 4 && a === b) matched = cursor;
    }

    if (matched >= 0) {
      out.push({ line: h.line, id: toc[matched].id, chunk: toc[matched].chunk });
      cursor = matched + 1;
    }
  }
  return out;
}
