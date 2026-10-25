// Configuração do MathJax 3 para as páginas de leitura. As macros do curso
// vêm no <div class="preamble"> de cada atividade (gerado pelo ximeraLatex);
// aqui ficam só as definições que o servidor Ximera acrescenta a toda página.
window.MathJax = {
  tex: {
    inlineMath: [['\\(', '\\)']],
    displayMath: [['\\[', '\\]']],
    processEnvironments: true,
    tags: 'ams',
    macros: {
      relax: '',
      xspace: '',
      ensuremath: ['{#1}', 1],
      arraycolsep: ['', 3],
      dd: ['\\frac{d #1}{d #2}', 2, ''],
      pp: ['\\frac{\\partial #1}{\\partial #2}', 2, ''],
      veci: '\\vec{\\hat{\\bf\\imath}}',
      vecj: '\\vec{\\hat{\\bf\\jmath}}',
      utan: '\\vec{\\hat{t}}',
      unormal: '\\vec{\\hat{n}}',
      // Um \answer que o pré-processamento não reconheceu mostra a resposta.
      answer: ['{#2}', 2, '']
    },
    environments: {
      amatrix: ['\\begin{bmatrix}', '\\end{bmatrix}', 1]
    }
  },
  chtml: {
    fontURL: '/static/ximera/mathjax/fonts/woff-v2'
  },
  options: {
    skipHtmlTags: ['script', 'noscript', 'style', 'textarea', 'pre', 'code', 'annotation', 'annotation-xml'],
    ignoreHtmlClass: 'tex2jax_ignore'
  }
};
