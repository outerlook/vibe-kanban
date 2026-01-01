import { createLowlight, common } from 'lowlight';

type SyntaxEntry = {
  value: string;
  lineNumber: number;
  valueLength: number;
  nodeList: Array<{ node: any; wrapper?: any }>;
};

type SyntaxResult = {
  syntaxFileObject: Record<number, SyntaxEntry>;
  syntaxFileLineNumber: number;
};

const processAST = (ast: any): SyntaxResult => {
  let lineNumber = 1;
  const syntaxObj: Record<number, SyntaxEntry> = {};

  const loopAST = (nodes: any[], wrapper?: any) => {
    nodes.forEach((node) => {
      if (node.type === 'text') {
        if (!node.value.includes('\n')) {
          const valueLength = node.value.length;
          if (!syntaxObj[lineNumber]) {
            node.startIndex = 0;
            node.endIndex = valueLength - 1;
            syntaxObj[lineNumber] = {
              value: node.value,
              lineNumber,
              valueLength,
              nodeList: [{ node, wrapper }],
            };
          } else {
            node.startIndex = syntaxObj[lineNumber].valueLength;
            node.endIndex = node.startIndex + valueLength - 1;
            syntaxObj[lineNumber].value += node.value;
            syntaxObj[lineNumber].valueLength += valueLength;
            syntaxObj[lineNumber].nodeList.push({ node, wrapper });
          }
          node.lineNumber = lineNumber;
          return;
        }

        const lines = node.value.split('\n');
        node.children = node.children || [];
        for (let i = 0; i < lines.length; i += 1) {
          const value = i === lines.length - 1 ? lines[i] : `${lines[i]}\n`;
          const currentLineNumber = i === 0 ? lineNumber : (lineNumber += 1);
          const valueLength = value.length;
          const childNode = {
            type: 'text',
            value,
            startIndex: Infinity,
            endIndex: Infinity,
            lineNumber: currentLineNumber,
          };

          if (!syntaxObj[currentLineNumber]) {
            childNode.startIndex = 0;
            childNode.endIndex = valueLength - 1;
            syntaxObj[currentLineNumber] = {
              value,
              lineNumber: currentLineNumber,
              valueLength,
              nodeList: [{ node: childNode, wrapper }],
            };
          } else {
            childNode.startIndex = syntaxObj[currentLineNumber].valueLength;
            childNode.endIndex = childNode.startIndex + valueLength - 1;
            syntaxObj[currentLineNumber].value += value;
            syntaxObj[currentLineNumber].valueLength += valueLength;
            syntaxObj[currentLineNumber].nodeList.push({
              node: childNode,
              wrapper,
            });
          }

          node.children.push(childNode);
        }

        node.lineNumber = lineNumber;
        return;
      }

      if (node.children) {
        loopAST(node.children, node);
        node.lineNumber = lineNumber;
      }
    });
  };

  loopAST(ast.children);
  return { syntaxFileObject: syntaxObj, syntaxFileLineNumber: lineNumber };
};

const _getAST = () => ({});

const lowlight = createLowlight(common) as {
  highlight: (lang: string, raw: string) => any;
  highlightAuto: (raw: string) => any;
  registered: (lang: string) => boolean;
  register: (name: string, lang: (hljs: any) => any) => void;
};

lowlight.register('vue', (hljs: any) => ({
  subLanguage: 'xml',
  contains: [
    hljs.COMMENT('<!--', '-->', {
      relevance: 10,
    }),
    {
      begin: /^(\s*)(<script>)/gm,
      end: /^(\s*)(<\/script>)/gm,
      subLanguage: 'javascript',
      excludeBegin: true,
      excludeEnd: true,
    },
    {
      begin: /^(?:\s*)(?:<script\s+lang=(["'])ts\1>)/gm,
      end: /^(\s*)(<\/script>)/gm,
      subLanguage: 'typescript',
      excludeBegin: true,
      excludeEnd: true,
    },
    {
      begin: /^(\s*)(<style(\s+scoped)?>)/gm,
      end: /^(\s*)(<\/style>)/gm,
      subLanguage: 'css',
      excludeBegin: true,
      excludeEnd: true,
    },
    {
      begin:
        /^(?:\s*)(?:<style(?:\s+scoped)?\s+lang=(["'])(?:s[ca]ss)\1(?:\s+scoped)?>)/gm,
      end: /^(\s*)(<\/style>)/gm,
      subLanguage: 'scss',
      excludeBegin: true,
      excludeEnd: true,
    },
    {
      begin:
        /^(?:\s*)(?:<style(?:\s+scoped)?\s+lang=(["'])stylus\1(?:\s+scoped)?>)/gm,
      end: /^(\s*)(<\/style>)/gm,
      subLanguage: 'stylus',
      excludeBegin: true,
      excludeEnd: true,
    },
  ],
}));

let maxLineToIgnoreSyntax = 2000;
const ignoreSyntaxHighlightList: Array<RegExp | string> = [];
const isDev = import.meta.env.DEV;

const highlighter = {
  name: 'lowlight',
  type: 'class',
  get maxLineToIgnoreSyntax() {
    return maxLineToIgnoreSyntax;
  },
  setMaxLineToIgnoreSyntax(value: number) {
    maxLineToIgnoreSyntax = value;
  },
  get ignoreSyntaxHighlightList() {
    return ignoreSyntaxHighlightList;
  },
  setIgnoreSyntaxHighlightList(value: Array<RegExp | string>) {
    ignoreSyntaxHighlightList.length = 0;
    ignoreSyntaxHighlightList.push(...value);
  },
  getAST(raw: string, fileName?: string, lang?: string) {
    let hasRegisteredLang = true;
    if (lang && !lowlight.registered(lang)) {
      if (isDev) {
        console.warn(`not support current lang: ${lang} yet`);
      }
      hasRegisteredLang = false;
    }

    if (
      fileName &&
      ignoreSyntaxHighlightList.some((item) =>
        item instanceof RegExp ? item.test(fileName) : fileName === item
      )
    ) {
      if (isDev) {
        console.warn(
          `ignore syntax for current file, because the fileName is in the ignoreSyntaxHighlightList: ${fileName}`
        );
      }
      return undefined;
    }

    if (lang && hasRegisteredLang) {
      return lowlight.highlight(lang, raw);
    }

    return lowlight.highlightAuto(raw);
  },
  processAST(ast: any) {
    return processAST(ast);
  },
  hasRegisteredCurrentLang(lang: string) {
    return lowlight.registered(lang);
  },
  getHighlighterEngine() {
    return lowlight;
  },
};

const versions = '0.0.30';

export { _getAST, highlighter, processAST, versions };
