// katex 的 contrib/auto-render 没有随包类型声明，这里补一份最小声明
declare module 'katex/contrib/auto-render' {
  interface RenderMathInElementOptions {
    delimiters?: { left: string; right: string; display: boolean }[];
    throwOnError?: boolean;
    ignoredTags?: string[];
    ignoredClasses?: string[];
    errorColor?: string;
    macros?: Record<string, string>;
  }
  export default function renderMathInElement(
    element: HTMLElement,
    options?: RenderMathInElementOptions
  ): void;
}
