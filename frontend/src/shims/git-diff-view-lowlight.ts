import { createLowlight, common } from 'lowlight';

type LowlightNode = {
  type?: string;
  value?: string;
  children?: LowlightNode[];
  startIndex?: number;
  endIndex?: number;
  lineNumber?: number;
};

type LowlightTextNode = LowlightNode & {
  type: 'text';
  value: string;
};

type LowlightRoot = {
  children: LowlightNode[];
};

type LowlightHljs = {
  COMMENT: (
    start: string,
    end: string,
    opts?: { relevance?: number }
  ) => unknown;
};

type LowlightLanguageDef = (hljs: LowlightHljs) => Record<string, unknown>;

type SyntaxEntry = {
  value: string;
  lineNumber: number;
  valueLength: number;
  nodeList: Array<{ node: LowlightNode; wrapper?: LowlightNode }>;
};

type SyntaxResult = {
  syntaxFileObject: Record<number, SyntaxEntry>;
  syntaxFileLineNumber: number;
};

const processAST = (ast: LowlightRoot): SyntaxResult => {
  let lineNumber = 1;
  const syntaxObj: Record<number, SyntaxEntry> = {};

  const loopAST = (nodes: LowlightNode[], wrapper?: LowlightNode) => {
    nodes.forEach((node) => {
      if (node.type === 'text') {
        const textNode = node as LowlightTextNode;
        if (!textNode.value.includes('\n')) {
          const valueLength = textNode.value.length;
          if (!syntaxObj[lineNumber]) {
            textNode.startIndex = 0;
            textNode.endIndex = valueLength - 1;
            syntaxObj[lineNumber] = {
              value: textNode.value,
              lineNumber,
              valueLength,
              nodeList: [{ node: textNode, wrapper }],
            };
          } else {
            textNode.startIndex = syntaxObj[lineNumber].valueLength;
            textNode.endIndex = textNode.startIndex + valueLength - 1;
            syntaxObj[lineNumber].value += textNode.value;
            syntaxObj[lineNumber].valueLength += valueLength;
            syntaxObj[lineNumber].nodeList.push({ node: textNode, wrapper });
          }
          textNode.lineNumber = lineNumber;
          return;
        }

        const lines = textNode.value.split('\n');
        textNode.children = textNode.children || [];
        for (let i = 0; i < lines.length; i += 1) {
          const value = i === lines.length - 1 ? lines[i] : `${lines[i]}\n`;
          const currentLineNumber = i === 0 ? lineNumber : (lineNumber += 1);
          const valueLength = value.length;
          const childNode: LowlightTextNode = {
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

          textNode.children.push(childNode);
        }

        textNode.lineNumber = lineNumber;
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

const lowlight = createLowlight(common) as unknown as {
  highlight: (lang: string, raw: string) => LowlightRoot;
  highlightAuto: (raw: string) => LowlightRoot;
  registered: (lang: string) => boolean;
  register: (name: string, lang: LowlightLanguageDef) => void;
};

lowlight.register('vue', (hljs: LowlightHljs) => ({
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
  processAST(ast: LowlightRoot) {
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
