import hljs from 'highlight.js/lib/core';

// 按需注册常用语言，把 1MB+ 全量包裁到 ~100KB（方案 §5.4）
import javascript from 'highlight.js/lib/languages/javascript';
import typescript from 'highlight.js/lib/languages/typescript';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import bash from 'highlight.js/lib/languages/bash';
import json from 'highlight.js/lib/languages/json';
import yaml from 'highlight.js/lib/languages/yaml';
import xml from 'highlight.js/lib/languages/xml';
import css from 'highlight.js/lib/languages/css';
import markdown from 'highlight.js/lib/languages/markdown';
import sql from 'highlight.js/lib/languages/sql';
import java from 'highlight.js/lib/languages/java';
import c from 'highlight.js/lib/languages/c';
import cpp from 'highlight.js/lib/languages/cpp';
import go from 'highlight.js/lib/languages/go';
import toml from 'highlight.js/lib/languages/ini';

// 语言名必须显式给出：模块导出的 .name 经打包器压缩后不可靠（方案附录 C #7）
hljs.registerLanguage('javascript', javascript);
hljs.registerLanguage('typescript', typescript);
hljs.registerLanguage('python', python);
hljs.registerLanguage('rust', rust);
hljs.registerLanguage('bash', bash);
hljs.registerLanguage('json', json);
hljs.registerLanguage('yaml', yaml);
hljs.registerLanguage('xml', xml);
hljs.registerLanguage('css', css);
hljs.registerLanguage('markdown', markdown);
hljs.registerLanguage('sql', sql);
hljs.registerLanguage('java', java);
hljs.registerLanguage('c', c);
hljs.registerLanguage('cpp', cpp);
hljs.registerLanguage('go', go);
hljs.registerLanguage('toml', toml);

// 别名一并注册，否则 ```js / ```sh / ```yml 不会高亮
hljs.registerAliases(['js', 'jsx'], { languageName: 'javascript' });
hljs.registerAliases(['ts', 'tsx'], { languageName: 'typescript' });
hljs.registerAliases(['py'], { languageName: 'python' });
hljs.registerAliases(['rs'], { languageName: 'rust' });
hljs.registerAliases(['sh', 'shell', 'zsh'], { languageName: 'bash' });
hljs.registerAliases(['yml'], { languageName: 'yaml' });
hljs.registerAliases(['html', 'vue', 'svelte'], { languageName: 'xml' });
hljs.registerAliases(['md'], { languageName: 'markdown' });
hljs.registerAliases(['c++', 'cc'], { languageName: 'cpp' });
hljs.registerAliases(['golang'], { languageName: 'go' });

/**
 * 只处理 scope 内未高亮过的代码块（方案 §5.1：增量增强，不能重扫全文）。
 * 不要用 hljs.highlightAll()。
 */
export function highlightWithin(scope: HTMLElement): void {
  scope.querySelectorAll<HTMLElement>('pre > code:not(.hljs)').forEach((el) => {
    if (el.classList.contains('language-mermaid')) return; // 跳过图表
    try {
      hljs.highlightElement(el);
    } catch {
      // 未知语言等异常情况：保持原样即可
    }
  });
}
