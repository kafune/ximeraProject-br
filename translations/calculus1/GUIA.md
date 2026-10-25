# Guia de tradução — Ximera Calculus 1 → português do Brasil

Você está traduzindo trechos de prosa de um livro interativo de Cálculo 1 (projeto Ximera / Mooculus, da Ohio State). O LaTeX já foi retirado e substituído por tokens como `{{MATH_7}}`, `{{CMD_12}}`, `{{ENV_3}}`, `{{COMMENT_5}}`, `{{PREAMBLE_2}}`. Você traduz SOMENTE a prosa entre os tokens.

## Formato de entrada

Cada segmento aparece assim:

    ### caminho/do/arquivo.tex #N
    [tokens] {{CMD_7}} = \item[Riley] ; {{MATH_11}} = $\sin(x)$ ; ...
    <<<
    texto original em inglês com tokens
    >>>

A linha `[tokens]` é só contexto para você entender o que cada token representa (às vezes truncado com `…`). Ela NÃO é copiada para a saída.

## Formato de saída (obrigatório)

Escreva UM arquivo de saída com todos os segmentos do lote, nesta forma exata, na mesma ordem da entrada:

    ### caminho/do/arquivo.tex #N
    texto traduzido com os mesmos tokens

    ### caminho/do/arquivo.tex #N+1
    texto traduzido

- A linha `### ... #N` é copiada exatamente da entrada (mesmo caminho, mesmo número).
- Depois dela vem apenas a tradução. Sem `<<<`, sem `>>>`, sem `[tokens]`, sem comentários seus, sem numeração extra, sem blocos de código Markdown.
- Nenhum segmento pode faltar. Nenhum pode ficar vazio.

## Regras invioláveis sobre tokens

1. Cada token da entrada aparece exatamente uma vez na saída, escrito caractere por caractere (`{{MATH_7}}`, nunca `{{MATH 7}}` ou `{{math_7}}`).
2. Os tokens aparecem na MESMA ORDEM relativa da entrada. Reorganize a frase em português se precisar, mas não troque tokens de lugar.
3. Não invente tokens, não apague tokens, não altere números de tokens.
4. Não escreva LaTeX novo (nada de `\emph`, `$...$`, `\textbf`). Toda a matemática e todos os comandos já estão nos tokens.
5. Muitos pares de tokens são aberturas e fechamentos, por exemplo `{{CMD_14}}` = `\textbf{` e `{{CMD_15}}` = `}`. O texto entre eles continua entre eles. Exemplo: `the derivative {{CMD_14}}is{{CMD_15}} the slope` → `a derivada {{CMD_14}}é{{CMD_15}} a inclinação`.
6. O primeiro segmento de cada arquivo costuma começar com o título da atividade seguido de `{{CMD_1}}` (que é o `}` do `\title{`). Traduza o título e mantenha o token logo depois: `Chain rule{{CMD_1}}` → `Regra da cadeia{{CMD_1}}`.
7. Mantenha a estrutura de linhas: tokens que estavam sozinhos numa linha continuam sozinhos numa linha; linhas em branco entre parágrafos continuam em branco. Espaços e tabulações no início das linhas podem ser mantidos como estão.
8. Em `\choice{...}` (alternativas de múltipla escolha) e `\wordChoice{\choice{...}\choice{...}}` (escolha de palavra dentro de uma frase), cada alternativa é prosa entre tokens: traduza todas, mantendo concordância com a frase em que se encaixam.
9. Se um segmento não tiver nada traduzível (só código, coordenadas, nomes de arquivo, ou texto que é claramente parte de um comando, como ` at (current page.center){{{CMD_1}}};`), copie-o sem alterações.

## Estilo

- Português do Brasil, ortografia do Acordo de 1990 (ideia, frequência, consequência, para, sequência).
- Registro de livro didático universitário, claro e amigável, fiel ao tom conversacional do original. Traduza o sentido, não palavra por palavra; a frase em português deve soar natural para um estudante brasileiro.
- O leitor é tratado por "você". Os autores falam em primeira pessoa do plural ("vamos", "podemos", "observamos"), como no original ("we").
- "Write with me" → "Escreva comigo". "Check out" → "Confira" / "Veja". "Armed with this fact" → "Munidos desse fato". "Let's" → "Vamos".
- Imperativos de exercícios: Compute → Calcule; Find → Determine (ou Encontre); Evaluate → Calcule; Simplify → Simplifique; Show that → Mostre que; Sketch → Esboce; Explain → Explique; Consider → Considere; Suppose → Suponha que; Let f be → Seja f; Recall that → Lembre-se de que; Note that → Observe que; Hence/Thus/Therefore → Logo / Portanto / Assim; That is → Isto é; In other words → Em outras palavras; For example → Por exemplo.
- Perguntas padrão: "Which of the following ..." → "Qual das alternativas a seguir ..."; "Select all that apply" → "Selecione todas as alternativas que se aplicam"; "True" / "False" → "Verdadeiro" / "Falso"; "none of the above" → "nenhuma das anteriores"; "impossible to say" → "impossível dizer"; "there is not enough information" → "não há informação suficiente"; "Yes" / "No" → "Sim" / "Não".
- Nomes próprios ficam: Devyn, Riley, Ximera, Mooculus, Newton, Leibniz, L'Hôpital, Riemann. Os diálogos entre Devyn e Riley são informais; mantenha-os leves e naturais.
- Títulos e seções em caixa baixa como no original (só a primeira letra maiúscula): "Derivatives of trigonometric functions" → "Derivadas das funções trigonométricas".
- Números: mantenha exatamente como no original, inclusive o ponto decimal (2.5 fica 2.5), porque as fórmulas nos tokens usam ponto.
- Unidades imperiais: traduza o nome sem converter (feet → pés, miles → milhas, inches → polegadas, miles per hour → milhas por hora, gallons → galões).
- Aspas: mantenha o estilo LaTeX ``assim'' quando o original usar; apóstrofo em inglês (`it's`) não existe em português, reescreva.
- Não traduza texto que esteja dentro de tokens. Não traduza nomes de arquivos, rótulos ou URLs.

## Glossário (use sempre estes termos)

| inglês | português |
| --- | --- |
| function | função |
| domain / range | domínio / imagem |
| input / output | entrada / saída |
| composition, composite function | composição, função composta |
| inverse function | função inversa |
| one-to-one | injetora (ou "um a um" quando o original explica a ideia) |
| graph / plot (noun) | gráfico |
| to plot / to graph | esboçar o gráfico de, traçar |
| slope | inclinação |
| tangent line / secant line | reta tangente / reta secante |
| limit | limite |
| one-sided limit; left-hand / right-hand limit | limite lateral; limite à esquerda / à direita |
| limit laws | leis dos limites (propriedades dos limites) |
| squeeze theorem | teorema do confronto |
| indeterminate form | forma indeterminada |
| asymptote (vertical, horizontal, slant) | assíntota (vertical, horizontal, oblíqua) |
| continuity, continuous | continuidade, contínua |
| removable / jump / infinite discontinuity | descontinuidade removível / de salto / infinita |
| intermediate value theorem | teorema do valor intermediário |
| extreme value theorem | teorema do valor extremo |
| derivative | derivada |
| differentiable | derivável (diferenciável também é aceito, mas prefira derivável) |
| to differentiate | derivar |
| rules of differentiation | regras de derivação |
| power rule / product rule / quotient rule / chain rule | regra da potência / regra do produto / regra do quociente / regra da cadeia |
| constant multiple rule / sum rule | regra do múltiplo constante / regra da soma |
| implicit differentiation | derivação implícita |
| logarithmic differentiation | derivação logarítmica |
| rate of change (average, instantaneous) | taxa de variação (média, instantânea) |
| related rates | taxas relacionadas |
| velocity / speed / acceleration | velocidade / velocidade escalar / aceleração |
| position / displacement | posição / deslocamento |
| higher-order derivatives | derivadas de ordem superior |
| second derivative | segunda derivada |
| increasing / decreasing | crescente / decrescente |
| concave up / concave down | concavidade para cima / concavidade para baixo (côncava para cima / para baixo) |
| inflection point | ponto de inflexão |
| critical point | ponto crítico |
| local (relative) maximum / minimum | máximo / mínimo local |
| global (absolute) maximum / minimum | máximo / mínimo absoluto (global) |
| extremum, extrema | extremo, extremos |
| first / second derivative test | teste da primeira / segunda derivada |
| mean value theorem | teorema do valor médio |
| Rolle's theorem | teorema de Rolle |
| linear approximation | aproximação linear |
| differential | diferencial |
| optimization | otimização |
| L'Hôpital's rule | regra de L'Hôpital |
| antiderivative | antiderivada |
| indefinite integral / definite integral | integral indefinida / integral definida |
| integrand | integrando |
| bounds (limits) of integration | limites de integração |
| Riemann sum | soma de Riemann |
| left / right / midpoint Riemann sum | soma de Riemann à esquerda / à direita / do ponto médio |
| area under the curve | área sob a curva |
| signed area / net area | área com sinal / área líquida |
| accumulation function | função acumulação |
| fundamental theorem of calculus (first, second) | teorema fundamental do cálculo (primeiro, segundo) |
| substitution (u-substitution) | substituição |
| average value | valor médio |
| polynomial / rational function | polinômio (função polinomial) / função racional |
| trigonometric / exponential / logarithmic function | função trigonométrica / exponencial / logarítmica |
| sine, cosine, tangent, secant, cosecant, cotangent | seno, cosseno, tangente, secante, cossecante, cotangente |
| inverse sine (arcsine) etc. | arco seno (seno inverso), arco cosseno, arco tangente |
| natural logarithm | logaritmo natural |
| even / odd function | função par / ímpar |
| piecewise function | função definida por partes |
| vertical line test / horizontal line test | teste da reta vertical / teste da reta horizontal |
| zoom in | ampliar (dar zoom) |
| real number / integer | número real / inteiro |
| interval (open, closed) | intervalo (aberto, fechado) |
| endpoint | extremidade (do intervalo) |
| theorem / lemma / corollary / definition / example / exercise / proof | teorema / lema / corolário / definição / exemplo / exercício / demonstração |
| hint | dica |
| explanation | explicação |
| feedback | comentário |
| (homework) problem | problema |
| question | pergunta |
| answer | resposta |
| break ground / dig in (nomes de seção, se aparecerem na prosa) | preparar o terreno / aprofundar |
| calculus | cálculo |
| two young mathematicians discuss ... | dois jovens matemáticos discutem ... |
